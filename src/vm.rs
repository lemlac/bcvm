//! The virtual machine: value stack, call frames, interpreter loop, and GC.

use std::rc::Rc;

use crate::chunk::OpCode;
use crate::common::{FRAMES_MAX, GC_HEAP_GROW_FACTOR, GC_INITIAL_THRESHOLD, STACK_MAX};
use crate::debug::disassemble_instruction;
use crate::table::Table;
use crate::value::{
    copy_string, new_native, NativeFn, Obj, ObjFunction, ObjRef, Value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpretResult {
    Ok,
    RuntimeError,
}

/// One activation record on the call stack.
struct CallFrame {
    function: ObjRef,
    /// Index into the function's chunk code (instruction pointer).
    ip: usize,
    /// Base index of this frame's locals inside the shared value stack.
    slots: usize,
}

/// The whole machine state.
pub struct Vm {
    frames: Vec<CallFrame>,
    stack: Vec<Value>,

    pub globals: Table,
    pub strings: Table, // interned string pool

    /// All live heap objects (for the sweep phase).
    objects: Vec<ObjRef>,

    bytes_allocated: usize,
    next_gc: usize,

    /// Gray stack for the mark phase.
    gray_stack: Vec<ObjRef>,

    /// When true, print stack + current instruction each step.
    pub trace_execution: bool,
}

impl Vm {
    pub fn new() -> Self {
        let mut vm = Vm {
            frames: Vec::with_capacity(FRAMES_MAX),
            stack: Vec::with_capacity(STACK_MAX),
            globals: Table::new(),
            strings: Table::new(),
            objects: Vec::new(),
            bytes_allocated: 0,
            next_gc: GC_INITIAL_THRESHOLD,
            gray_stack: Vec::new(),
            trace_execution: false,
        };
        vm.define_native("clock", clock_native);
        vm
    }

    // ------------------------------------------------------------------
    // Stack helpers
    // ------------------------------------------------------------------

    pub fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    pub fn pop(&mut self) -> Value {
        self.stack.pop().expect("stack underflow")
    }

    pub fn peek(&self, distance: usize) -> &Value {
        &self.stack[self.stack.len() - 1 - distance]
    }

    fn reset_stack(&mut self) {
        self.stack.clear();
        self.frames.clear();
    }

    // ------------------------------------------------------------------
    // Object tracking / GC bookkeeping
    // ------------------------------------------------------------------

    pub fn track_object(&mut self, obj: ObjRef) {
        // Approximate size tracking (good enough for the demo).
        let size = match &*obj.borrow() {
            Obj::String(s) => std::mem::size_of::<Obj>() + s.chars.len(),
            Obj::Function(_) => std::mem::size_of::<ObjFunction>(),
            Obj::Native(_) => std::mem::size_of::<crate::value::ObjNative>(),
        };
        self.bytes_allocated += size;
        self.objects.push(obj);

        #[cfg(feature = "gc_stress")]
        {
            self.collect_garbage();
        }
        #[cfg(not(feature = "gc_stress"))]
        {
            if self.bytes_allocated > self.next_gc {
                self.collect_garbage();
            }
        }
    }

    // ------------------------------------------------------------------
    // Natives
    // ------------------------------------------------------------------

    pub fn define_native(&mut self, name: &'static str, function: NativeFn) {
        let name_obj = copy_string(self, name);
        let native = new_native(self, function, name);
        self.globals.set(name_obj, Value::Obj(native));
    }

    // ------------------------------------------------------------------
    // Runtime error
    // ------------------------------------------------------------------

    fn runtime_error(&mut self, msg: &str) {
        eprintln!("{}", msg);
        for frame in self.frames.iter().rev() {
            let fn_ = frame.function.borrow();
            let f = fn_.as_function();
            let instr = frame.ip.saturating_sub(1);
            let line = f.chunk.lines.get(instr).copied().unwrap_or(0);
            let name = f
                .name
                .as_ref()
                .map(|n| n.borrow().as_string().chars.clone())
                .unwrap_or_else(|| "<script>".to_string());
            eprintln!("  [line {}] in {}", line, name);
        }
        self.reset_stack();
    }

    // ------------------------------------------------------------------
    // Calling convention
    // ------------------------------------------------------------------

    fn call(&mut self, function: ObjRef, arg_count: usize) -> bool {
        let arity = {
            let fn_ = function.borrow();
            fn_.as_function().arity as usize
        };
        if arg_count != arity {
            self.runtime_error(&format!(
                "expected {} argument(s) but got {}",
                arity, arg_count
            ));
            return false;
        }
        if self.frames.len() == FRAMES_MAX {
            self.runtime_error("stack overflow");
            return false;
        }
        let slots = self.stack.len() - arg_count - 1;
        self.frames.push(CallFrame {
            function,
            ip: 0,
            slots,
        });
        true
    }

    fn call_value(&mut self, callee: Value, arg_count: usize) -> bool {
        match callee {
            Value::Obj(ref o) => {
                let kind = {
                    // We need to decide without holding the borrow across the call.
                    let borrowed = o.borrow();
                    match &*borrowed {
                        Obj::Function(_) => 0,
                        Obj::Native(_) => 1,
                        _ => 2,
                    }
                };
                match kind {
                    0 => self.call(Rc::clone(o), arg_count),
                    1 => {
                        let native_fn = {
                            let borrowed = o.borrow();
                            borrowed.as_native().function
                        };
                        let args_start = self.stack.len() - arg_count;
                        let result = native_fn(arg_count, &self.stack[args_start..]);
                        // Pop args + the callee itself.
                        self.stack.truncate(self.stack.len() - arg_count - 1);
                        self.push(result);
                        true
                    }
                    _ => {
                        self.runtime_error(&format!(
                            "can only call functions (got a {})",
                            callee.type_name()
                        ));
                        false
                    }
                }
            }
            _ => {
                self.runtime_error(&format!(
                    "can only call functions (got a {})",
                    callee.type_name()
                ));
                false
            }
        }
    }

    fn is_falsey(v: &Value) -> bool {
        matches!(v, Value::Nil | Value::Bool(false))
    }

    fn concatenate(&mut self) {
        let b = self.peek(0).as_string();
        let a = self.peek(1).as_string();
        let combined = format!("{}{}", a.chars, b.chars);
        // Drop the two operands first so the GC can see the new string as the only root.
        self.pop();
        self.pop();
        let result = copy_string(self, &combined);
        self.push(Value::Obj(result));
    }

    // ------------------------------------------------------------------
    // Interpreter loop
    // ------------------------------------------------------------------

    pub fn interpret(&mut self, entry: ObjRef) -> InterpretResult {
        self.push(Value::Obj(Rc::clone(&entry)));
        if !self.call(entry, 0) {
            return InterpretResult::RuntimeError;
        }
        self.run()
    }

    fn run(&mut self) -> InterpretResult {
        loop {
            let frame_idx = self.frames.len() - 1;

            if self.trace_execution {
                print!("          ");
                for v in &self.stack {
                    print!("[ {} ]", v);
                }
                println!();
                let ip = self.frames[frame_idx].ip;
                // Borrow the function for the duration of disassembly only.
                let fn_ = self.frames[frame_idx].function.borrow();
                disassemble_instruction(&fn_.as_function().chunk, ip);
                drop(fn_);
            }

            // Read the next instruction.
            let instruction = {
                let frame = &mut self.frames[frame_idx];
                let fn_ = frame.function.borrow();
                let chunk = &fn_.as_function().chunk;
                let byte = chunk.code[frame.ip];
                frame.ip += 1;
                OpCode::from(byte)
            };

            match instruction {
                OpCode::Const => {
                    let idx = self.read_byte(frame_idx) as usize;
                    let constant = {
                        let frame = &self.frames[frame_idx];
                        let fn_ = frame.function.borrow();
                        fn_.as_function().chunk.constants.values[idx].clone()
                    };
                    self.push(constant);
                }
                OpCode::Nil => self.push(Value::Nil),
                OpCode::True => self.push(Value::Bool(true)),
                OpCode::False => self.push(Value::Bool(false)),
                OpCode::Pop => {
                    self.pop();
                }
                OpCode::Dup => {
                    let v = self.peek(0).clone();
                    self.push(v);
                }

                OpCode::GetLocal => {
                    let slot = self.read_byte(frame_idx) as usize;
                    let base = self.frames[frame_idx].slots;
                    let v = self.stack[base + slot].clone();
                    self.push(v);
                }
                OpCode::SetLocal => {
                    let slot = self.read_byte(frame_idx) as usize;
                    let base = self.frames[frame_idx].slots;
                    self.stack[base + slot] = self.peek(0).clone();
                }

                OpCode::GetGlobal => {
                    let name = self.read_string(frame_idx);
                    match self.globals.get(&name) {
                        Some(v) => self.push(v),
                        None => {
                            let n = name.borrow().as_string().chars.clone();
                            self.runtime_error(&format!("undefined variable '{}'", n));
                            return InterpretResult::RuntimeError;
                        }
                    }
                }
                OpCode::DefineGlobal => {
                    let name = self.read_string(frame_idx);
                    let value = self.peek(0).clone();
                    self.globals.set(name, value);
                    self.pop();
                }
                OpCode::SetGlobal => {
                    let name = self.read_string(frame_idx);
                    let value = self.peek(0).clone();
                    if self.globals.set(Rc::clone(&name), value) {
                        // set returned true → key was new → undefined
                        self.globals.delete(&name);
                        let n = name.borrow().as_string().chars.clone();
                        self.runtime_error(&format!("undefined variable '{}'", n));
                        return InterpretResult::RuntimeError;
                    }
                }

                OpCode::Add => {
                    if self.peek(0).is_string() && self.peek(1).is_string() {
                        self.concatenate();
                    } else if self.peek(0).is_number() && self.peek(1).is_number() {
                        let b = self.pop().as_number();
                        let a = self.pop().as_number();
                        self.push(Value::Number(a + b));
                    } else {
                        self.runtime_error("operands must be two numbers or two strings");
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Sub => {
                    if let Some(r) = self.binary_numeric(|a, b| a - b) {
                        self.push(Value::Number(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Mul => {
                    if let Some(r) = self.binary_numeric(|a, b| a * b) {
                        self.push(Value::Number(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Div => {
                    if let Some(r) = self.binary_numeric(|a, b| a / b) {
                        self.push(Value::Number(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Mod => {
                    if let Some(r) = self.binary_numeric(|a, b| a % b) {
                        self.push(Value::Number(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Negate => {
                    if !self.peek(0).is_number() {
                        self.runtime_error("operand must be a number");
                        return InterpretResult::RuntimeError;
                    }
                    let n = self.pop().as_number();
                    self.push(Value::Number(-n));
                }
                OpCode::Not => {
                    let v = self.pop();
                    self.push(Value::Bool(Self::is_falsey(&v)));
                }

                OpCode::Equal => {
                    let b = self.pop();
                    let a = self.pop();
                    self.push(Value::Bool(a == b));
                }
                OpCode::Greater => {
                    if let Some(r) = self.binary_numeric(|a, b| a > b) {
                        self.push(Value::Bool(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }
                OpCode::Less => {
                    if let Some(r) = self.binary_numeric(|a, b| a < b) {
                        self.push(Value::Bool(r));
                    } else {
                        return InterpretResult::RuntimeError;
                    }
                }

                OpCode::Print => {
                    let v = self.pop();
                    println!("{}", v);
                }

                OpCode::Jump => {
                    let offset = self.read_short(frame_idx) as usize;
                    self.frames[frame_idx].ip += offset;
                }
                OpCode::JumpIfFalse => {
                    let offset = self.read_short(frame_idx) as usize;
                    if Self::is_falsey(self.peek(0)) {
                        self.frames[frame_idx].ip += offset;
                    }
                }
                OpCode::Loop => {
                    let offset = self.read_short(frame_idx) as usize;
                    self.frames[frame_idx].ip -= offset;
                }

                OpCode::Call => {
                    let arg_count = self.read_byte(frame_idx) as usize;
                    let callee = self.peek(arg_count).clone();
                    if !self.call_value(callee, arg_count) {
                        return InterpretResult::RuntimeError;
                    }
                    // After a successful call the current frame may have changed.
                }
                OpCode::Return => {
                    let result = self.pop();
                    let frame = self.frames.pop().unwrap();
                    if self.frames.is_empty() {
                        self.pop(); // the top-level script function value
                        return InterpretResult::Ok;
                    }
                    self.stack.truncate(frame.slots);
                    self.push(result);
                }

                OpCode::Halt => return InterpretResult::Ok,
            }
        }
    }

    // ------------------------------------------------------------------
    // Bytecode readers (operate on the current frame)
    // ------------------------------------------------------------------

    fn read_byte(&mut self, frame_idx: usize) -> u8 {
        let frame = &mut self.frames[frame_idx];
        let fn_ = frame.function.borrow();
        let byte = fn_.as_function().chunk.code[frame.ip];
        // We can't mutate ip while the borrow is alive, so drop it first.
        drop(fn_);
        self.frames[frame_idx].ip += 1;
        byte
    }

    fn read_short(&mut self, frame_idx: usize) -> u16 {
        let b1 = self.read_byte(frame_idx) as u16;
        let b2 = self.read_byte(frame_idx) as u16;
        (b1 << 8) | b2
    }

    fn read_string(&mut self, frame_idx: usize) -> ObjRef {
        let idx = self.read_byte(frame_idx) as usize;
        let frame = &self.frames[frame_idx];
        let fn_ = frame.function.borrow();
        let constant = &fn_.as_function().chunk.constants.values[idx];
        match constant {
            Value::Obj(o) => Rc::clone(o),
            _ => panic!("constant is not a string"),
        }
    }

    fn binary_numeric<T, F>(&mut self, op: F) -> Option<T>
    where
        F: FnOnce(f64, f64) -> T,
    {
        if !self.peek(0).is_number() || !self.peek(1).is_number() {
            self.runtime_error("operands must be numbers");
            return None;
        }
        let b = self.pop().as_number();
        let a = self.pop().as_number();
        Some(op(a, b))
    }

    // ------------------------------------------------------------------
    // Mark-sweep garbage collector
    // ------------------------------------------------------------------

    pub fn collect_garbage(&mut self) {
        #[cfg(feature = "gc_log")]
        {
            eprintln!("-- gc begin");
            let before = self.bytes_allocated;
        }

        self.mark_roots();
        self.trace_references();
        self.strings.remove_white();
        self.sweep();

        self.next_gc = self.bytes_allocated * GC_HEAP_GROW_FACTOR;

        #[cfg(feature = "gc_log")]
        {
            eprintln!(
                "-- gc end   collected {} bytes ({} -> {}), next at {}",
                before - self.bytes_allocated,
                before,
                self.bytes_allocated,
                self.next_gc
            );
        }
    }

    fn mark_roots(&mut self) {
        // Collect roots first to avoid simultaneous borrows of `self`.
        // Reachability is: value stack → open call frames → globals table.
        // Function constant pools are traced via blacken when a function is marked.
        let mut roots: Vec<ObjRef> = Vec::new();

        for v in &self.stack {
            if let Value::Obj(o) = v {
                roots.push(Rc::clone(o));
            }
        }
        for frame in &self.frames {
            roots.push(Rc::clone(&frame.function));
        }

        // Globals (keys + values)
        self.globals.mark(&mut |o| roots.push(o));

        for o in roots {
            self.mark_object(o);
        }
    }

    fn mark_object(&mut self, obj: ObjRef) {
        if obj.borrow().is_marked() {
            return;
        }
        #[cfg(feature = "gc_log")]
        {
            eprint!("{:p} mark ", Rc::as_ptr(&obj));
            eprintln!("{}", obj.borrow());
        }
        obj.borrow_mut().set_marked(true);
        self.gray_stack.push(obj);
    }

    fn trace_references(&mut self) {
        while let Some(obj) = self.gray_stack.pop() {
            // Collect outgoing references without holding the borrow.
            let mut children: Vec<ObjRef> = Vec::new();
            {
                let borrowed = obj.borrow();
                match &*borrowed {
                    Obj::Function(f) => {
                        if let Some(ref name) = f.name {
                            children.push(Rc::clone(name));
                        }
                        for v in &f.chunk.constants.values {
                            if let Value::Obj(o) = v {
                                children.push(Rc::clone(o));
                            }
                        }
                    }
                    Obj::String(_) | Obj::Native(_) => {}
                }
            }
            for c in children {
                self.mark_object(c);
            }
        }
    }

    fn sweep(&mut self) {
        let mut i = 0;
        while i < self.objects.len() {
            if self.objects[i].borrow().is_marked() {
                self.objects[i].borrow_mut().set_marked(false);
                i += 1;
            } else {
                // Approximate size accounting.
                let size = match &*self.objects[i].borrow() {
                    Obj::String(s) => std::mem::size_of::<Obj>() + s.chars.len(),
                    Obj::Function(_) => std::mem::size_of::<ObjFunction>(),
                    Obj::Native(_) => std::mem::size_of::<crate::value::ObjNative>(),
                };
                self.bytes_allocated = self.bytes_allocated.saturating_sub(size);
                self.objects.swap_remove(i);
                // Note: the Rc will be dropped when no other references remain.
            }
        }
    }

    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }
}

// ---------------------------------------------------------------------------
// Built-in native
// ---------------------------------------------------------------------------

fn clock_native(_arg_count: usize, _args: &[Value]) -> Value {
    // We can't access the Vm's Instant from here easily without making the
    // native signature more complex; a simple process-time approximation
    // is fine for the demo (mirrors the C clock()/CLOCKS_PER_SEC).
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Value::Number(secs)
}

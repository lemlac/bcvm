//! Tagged values and heap objects.
//!
//! `Value` is a small tagged union (nil / bool / number / object pointer).
//! Every heap object shares an `Obj` header so the GC can walk the heap
//! uniformly.

use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;

use crate::chunk::Chunk;
use crate::vm::Vm;

/// Runtime type tag for a Value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Nil,
    Bool,
    Number,
    Obj,
}

/// A tagged value that lives on the stack / in constant pools / in tables.
#[derive(Debug, Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    Obj(ObjRef),
}

impl Value {
    #[inline]
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    #[inline]
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }

    #[inline]
    pub fn is_obj(&self) -> bool {
        matches!(self, Value::Obj(_))
    }

    #[inline]
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => panic!("Value is not a bool"),
        }
    }

    #[inline]
    pub fn as_number(&self) -> f64 {
        match self {
            Value::Number(n) => *n,
            _ => panic!("Value is not a number"),
        }
    }

    #[inline]
    pub fn as_obj(&self) -> &ObjRef {
        match self {
            Value::Obj(o) => o,
            _ => panic!("Value is not an object"),
        }
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::Obj(o) if matches!(*o.borrow(), Obj::String(_)))
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Value::Obj(o) if matches!(*o.borrow(), Obj::Function(_)))
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Value::Obj(o) if matches!(*o.borrow(), Obj::Native(_)))
    }

    pub fn as_string(&self) -> ObjString {
        match self {
            Value::Obj(o) => match &*o.borrow() {
                Obj::String(s) => s.clone(),
                _ => panic!("not a string"),
            },
            _ => panic!("not an object"),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::Obj(o) => match &*o.borrow() {
                Obj::String(_) => "string",
                Obj::Function(_) => "function",
                Obj::Native(_) => "native",
            },
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            // Strings are interned, so pointer equality is content equality.
            (Value::Obj(a), Value::Obj(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Value::Number(n) => {
                // Match the C %g behaviour reasonably closely.
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{:.0}", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Obj(o) => write!(f, "{}", o.borrow()),
        }
    }
}

// ---------------------------------------------------------------------------
// Heap objects
// ---------------------------------------------------------------------------

/// Shared reference to a heap object (the GC root set holds these).
pub type ObjRef = Rc<RefCell<Obj>>;

/// Concrete heap object kinds.
#[derive(Debug)]
pub enum Obj {
    String(ObjString),
    Function(ObjFunction),
    Native(ObjNative),
}

impl Obj {
    pub fn is_marked(&self) -> bool {
        match self {
            Obj::String(s) => s.is_marked,
            Obj::Function(f) => f.is_marked,
            Obj::Native(n) => n.is_marked,
        }
    }

    pub fn set_marked(&mut self, marked: bool) {
        match self {
            Obj::String(s) => s.is_marked = marked,
            Obj::Function(f) => f.is_marked = marked,
            Obj::Native(n) => n.is_marked = marked,
        }
    }

    pub fn as_string(&self) -> &ObjString {
        match self {
            Obj::String(s) => s,
            _ => panic!("not a string"),
        }
    }

    pub fn as_function(&self) -> &ObjFunction {
        match self {
            Obj::Function(f) => f,
            _ => panic!("not a function"),
        }
    }

    pub fn as_native(&self) -> &ObjNative {
        match self {
            Obj::Native(n) => n,
            _ => panic!("not a native"),
        }
    }
}

impl fmt::Display for Obj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Obj::String(s) => write!(f, "{}", s.chars),
            Obj::Function(fn_) => {
                if let Some(ref name) = fn_.name {
                    write!(f, "<fn {}/{}>", name.borrow().as_string().chars, fn_.arity)
                } else {
                    write!(f, "<script>")
                }
            }
            Obj::Native(n) => write!(f, "<native {}>", n.name),
        }
    }
}

/// Interned string object.  The characters live inside the object so the
/// GC can free the whole thing in one go.
#[derive(Debug, Clone)]
pub struct ObjString {
    pub is_marked: bool,
    pub length: usize,
    pub hash: u32,
    pub chars: String,
}

impl ObjString {
    pub fn new(chars: String, hash: u32) -> Self {
        let length = chars.len();
        ObjString {
            is_marked: false,
            length,
            hash,
            chars,
        }
    }
}

/// Function object: arity + its own Chunk + optional name.
#[derive(Debug)]
pub struct ObjFunction {
    pub is_marked: bool,
    pub arity: u8,
    pub chunk: Chunk,
    pub name: Option<ObjRef>,
}

impl ObjFunction {
    pub fn new() -> Self {
        ObjFunction {
            is_marked: false,
            arity: 0,
            chunk: Chunk::new(),
            name: None,
        }
    }
}

/// Native (host) function wrapper.
pub type NativeFn = fn(arg_count: usize, args: &[Value]) -> Value;

#[derive(Debug)]
pub struct ObjNative {
    pub is_marked: bool,
    pub function: NativeFn,
    pub name: &'static str,
}

impl ObjNative {
    pub fn new(function: NativeFn, name: &'static str) -> Self {
        ObjNative {
            is_marked: false,
            function,
            name,
        }
    }
}

// ---------------------------------------------------------------------------
// Allocation helpers (called from the VM)
// ---------------------------------------------------------------------------

/// FNV-1a hash (identical to the C implementation).
pub fn hash_string(key: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for &b in key.as_bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// Allocate a new string, interning it if an identical one already exists.
pub fn copy_string(vm: &mut Vm, chars: &str) -> ObjRef {
    let hash = hash_string(chars);
    if let Some(existing) = vm.strings.find_string(chars, hash) {
        return existing;
    }

    let s = ObjString::new(chars.to_owned(), hash);
    let obj = Rc::new(RefCell::new(Obj::String(s)));

    // Protect from GC while we insert into the intern table.
    vm.push(Value::Obj(Rc::clone(&obj)));
    vm.strings.set(Rc::clone(&obj), Value::Nil);
    vm.pop();

    vm.track_object(Rc::clone(&obj));
    obj
}

/// Create a fresh function object.
pub fn new_function(vm: &mut Vm) -> ObjRef {
    let fn_ = ObjFunction::new();
    let obj = Rc::new(RefCell::new(Obj::Function(fn_)));
    vm.track_object(Rc::clone(&obj));
    obj
}

/// Create a native-function wrapper.
pub fn new_native(vm: &mut Vm, function: NativeFn, name: &'static str) -> ObjRef {
    let n = ObjNative::new(function, name);
    let obj = Rc::new(RefCell::new(Obj::Native(n)));
    vm.track_object(Rc::clone(&obj));
    obj
}

/// Growable array of Values (used for constant pools).
#[derive(Debug, Default)]
pub struct ValueArray {
    pub values: Vec<Value>,
}

impl ValueArray {
    pub fn new() -> Self {
        ValueArray { values: Vec::new() }
    }

    pub fn write(&mut self, value: Value) -> usize {
        let idx = self.values.len();
        self.values.push(value);
        idx
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

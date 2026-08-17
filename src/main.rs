//! Demo driver – builds three bytecode programs by hand (the role a real
//! compiler front-end would play) and runs them.

use std::env;

use bcvm::chunk::OpCode;
use bcvm::debug::disassemble_chunk;
use bcvm::value::{copy_string, new_function, Obj, Value};
use bcvm::vm::{InterpretResult, Vm};

fn name(vm: &mut Vm, s: &str) -> bcvm::value::ObjRef {
    copy_string(vm, s)
}

// ---------------------------------------------------------------------------
// Program 1: recursive Fibonacci
//
// fn fib(n) {
//     if (n < 2) return n;
//     return fib(n - 1) + fib(n - 2);
// }
// print fib(21);
// ---------------------------------------------------------------------------
fn build_fib(vm: &mut Vm) -> bcvm::value::ObjRef {
    let fn_obj = new_function(vm);
    {
        let mut fn_ = fn_obj.borrow_mut();
        let f = match &mut *fn_ {
            Obj::Function(f) => f,
            _ => unreachable!(),
        };
        f.arity = 1;
        f.name = Some(name(vm, "fib"));

        let c = &mut f.chunk;

        // if (n < 2) return n;
        c.emit_bytes(OpCode::GetLocal as u8, 1, 1); // n
        c.emit_constant(Value::Number(2.0), 1);
        c.emit_op(OpCode::Less, 1);
        let then_jump = c.emit_jump(OpCode::JumpIfFalse, 1);
        c.emit_op(OpCode::Pop, 1);
        c.emit_bytes(OpCode::GetLocal as u8, 1, 1);
        c.emit_op(OpCode::Return, 1);
        let else_jump = c.emit_jump(OpCode::Jump, 1);
        c.patch_jump(then_jump);
        c.emit_op(OpCode::Pop, 1);

        // return fib(n-1) + fib(n-2);
        let fib_name = name(vm, "fib");
        let fib_name_idx = c.add_constant(Value::Obj(fib_name));

        c.emit_bytes(OpCode::GetGlobal as u8, fib_name_idx as u8, 2);
        c.emit_bytes(OpCode::GetLocal as u8, 1, 2);
        c.emit_constant(Value::Number(1.0), 2);
        c.emit_op(OpCode::Sub, 2);
        c.emit_bytes(OpCode::Call as u8, 1, 2);

        c.emit_bytes(OpCode::GetGlobal as u8, fib_name_idx as u8, 2);
        c.emit_bytes(OpCode::GetLocal as u8, 1, 2);
        c.emit_constant(Value::Number(2.0), 2);
        c.emit_op(OpCode::Sub, 2);
        c.emit_bytes(OpCode::Call as u8, 1, 2);

        c.emit_op(OpCode::Add, 2);
        c.emit_op(OpCode::Return, 2);

        c.patch_jump(else_jump);
        c.emit_op(OpCode::Nil, 3);
        c.emit_op(OpCode::Return, 3);
    }
    fn_obj
}

fn build_fib_script(vm: &mut Vm, arg: i32) -> bcvm::value::ObjRef {
    let script = new_function(vm);
    {
        let mut fn_ = script.borrow_mut();
        let f = match &mut *fn_ {
            Obj::Function(f) => f,
            _ => unreachable!(),
        };
        f.name = None;
        let c = &mut f.chunk;

        let fib = build_fib(vm);
        c.emit_constant(Value::Obj(fib), 1);
        let fib_name = name(vm, "fib");
        let fib_name_idx = c.add_constant(Value::Obj(fib_name));
        c.emit_bytes(OpCode::DefineGlobal as u8, fib_name_idx as u8, 1);

        c.emit_bytes(OpCode::GetGlobal as u8, fib_name_idx as u8, 2);
        c.emit_constant(Value::Number(arg as f64), 2);
        c.emit_bytes(OpCode::Call as u8, 1, 2);
        c.emit_op(OpCode::Print, 2);

        c.emit_op(OpCode::Nil, 3);
        c.emit_op(OpCode::Return, 3);
    }
    script
}

// ---------------------------------------------------------------------------
// Program 2: iterative sum 1..n
//
// var i = 1; var sum = 0;
// while (i <= n) { sum = sum + i; i = i + 1; }
// print sum;
// ---------------------------------------------------------------------------
fn build_sum_loop(vm: &mut Vm, n: i32) -> bcvm::value::ObjRef {
    let script = new_function(vm);
    {
        let mut fn_ = script.borrow_mut();
        let f = match &mut *fn_ {
            Obj::Function(f) => f,
            _ => unreachable!(),
        };
        f.name = None;
        let c = &mut f.chunk;

        // locals: slot 0 = script itself, slot 1 = i, slot 2 = sum
        c.emit_constant(Value::Number(1.0), 1); // i = 1
        c.emit_constant(Value::Number(0.0), 1); // sum = 0

        let loop_start = c.count();
        // condition: NOT (i > n) ≡ i <= n
        c.emit_bytes(OpCode::GetLocal as u8, 1, 2);
        c.emit_constant(Value::Number(n as f64), 2);
        c.emit_op(OpCode::Greater, 2);
        c.emit_op(OpCode::Not, 2);
        let exit_jump = c.emit_jump(OpCode::JumpIfFalse, 2);
        c.emit_op(OpCode::Pop, 2);

        // sum = sum + i
        c.emit_bytes(OpCode::GetLocal as u8, 2, 3);
        c.emit_bytes(OpCode::GetLocal as u8, 1, 3);
        c.emit_op(OpCode::Add, 3);
        c.emit_bytes(OpCode::SetLocal as u8, 2, 3);
        c.emit_op(OpCode::Pop, 3);

        // i = i + 1
        c.emit_bytes(OpCode::GetLocal as u8, 1, 4);
        c.emit_constant(Value::Number(1.0), 4);
        c.emit_op(OpCode::Add, 4);
        c.emit_bytes(OpCode::SetLocal as u8, 1, 4);
        c.emit_op(OpCode::Pop, 4);

        c.emit_loop(loop_start, 4);

        c.patch_jump(exit_jump);
        c.emit_op(OpCode::Pop, 5);

        c.emit_bytes(OpCode::GetLocal as u8, 2, 6);
        c.emit_op(OpCode::Print, 6);

        c.emit_op(OpCode::Nil, 7);
        c.emit_op(OpCode::Return, 7);
    }
    script
}

// ---------------------------------------------------------------------------
// Program 3: GC stress – many short-lived string concatenations
// ---------------------------------------------------------------------------
fn build_gc_demo(vm: &mut Vm, iterations: i32) -> bcvm::value::ObjRef {
    let script = new_function(vm);
    {
        let mut fn_ = script.borrow_mut();
        let f = match &mut *fn_ {
            Obj::Function(f) => f,
            _ => unreachable!(),
        };
        f.name = None;
        let c = &mut f.chunk;

        c.emit_constant(Value::Number(0.0), 1); // i (slot 1)

        let loop_start = c.count();
        c.emit_bytes(OpCode::GetLocal as u8, 1, 2);
        c.emit_constant(Value::Number(iterations as f64), 2);
        c.emit_op(OpCode::Less, 2);
        let exit = c.emit_jump(OpCode::JumpIfFalse, 2);
        c.emit_op(OpCode::Pop, 2);

        // throw-away "garbage-" + "chunk"
        let g1 = name(vm, "garbage-");
        let g2 = name(vm, "chunk");
        c.emit_constant(Value::Obj(g1), 3);
        c.emit_constant(Value::Obj(g2), 3);
        c.emit_op(OpCode::Add, 3);
        c.emit_op(OpCode::Pop, 3);

        // i = i + 1
        c.emit_bytes(OpCode::GetLocal as u8, 1, 4);
        c.emit_constant(Value::Number(1.0), 4);
        c.emit_op(OpCode::Add, 4);
        c.emit_bytes(OpCode::SetLocal as u8, 1, 4);
        c.emit_op(OpCode::Pop, 4);

        c.emit_loop(loop_start, 4);
        c.patch_jump(exit);
        c.emit_op(OpCode::Pop, 5);

        let msg = name(vm, "gc demo done");
        let msg_idx = c.add_constant(Value::Obj(msg));
        c.emit_bytes(OpCode::Const as u8, msg_idx as u8, 6);
        c.emit_op(OpCode::Print, 6);

        c.emit_op(OpCode::Nil, 7);
        c.emit_op(OpCode::Return, 7);
    }
    script
}

fn run_program(vm: &mut Vm, title: &str, script: bcvm::value::ObjRef, trace: bool) {
    println!("\n===== {} =====", title);
    if trace {
        let fn_ = script.borrow();
        disassemble_chunk(&fn_.as_function().chunk, title);
    }
    println!("-- output --");
    let result = vm.interpret(script);
    if result == InterpretResult::RuntimeError {
        println!("(runtime error)");
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let trace = args.iter().any(|a| a == "--trace");

    let mut vm = Vm::new();
    vm.trace_execution = trace;

    let fib_script = build_fib_script(&mut vm, 21);
    run_program(&mut vm, "fib(21) via recursive calls", fib_script, trace);

    let sum_script = build_sum_loop(&mut vm, 100);
    run_program(&mut vm, "sum 1..100 via a while-loop", sum_script, trace);

    println!("\n===== gc demo (200,000 throwaway string concats) =====");
    println!("bytesAllocated before:              {:8}", vm.bytes_allocated());
    let gc_script = build_gc_demo(&mut vm, 200_000);
    let r = vm.interpret(gc_script);
    if r == InterpretResult::RuntimeError {
        println!("(runtime error)");
    }
    println!(
        "bytesAllocated right after loop:    {:8}  (heap-tracking GC already\n\
         \x20                                           ran automatically mid-loop\n\
         \x20                                           as soon as the threshold was hit)",
        vm.bytes_allocated()
    );
    vm.collect_garbage();
    println!(
        "bytesAllocated after explicit GC:   {:8}",
        vm.bytes_allocated()
    );
}

// ---------------------------------------------------------------------------
// Unit tests — one per demo program, asserting on real results (via
// `Vm::output()` and `Vm::bytes_allocated()`) instead of eyeballing stdout.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fib_21_via_recursive_calls() {
        let mut vm = Vm::new();
        let script = build_fib_script(&mut vm, 21);
        let result = vm.interpret(script);

        assert_eq!(result, InterpretResult::Ok);
        assert_eq!(vm.output(), ["10946".to_string()]);
    }

    #[test]
    fn fib_matches_known_sequence() {
        // A couple of extra points on the curve, cheap insurance against an
        // off-by-one in the base case or the recursive step individually.
        for (n, expected) in [(0, "0"), (1, "1"), (10, "55")] {
            let mut vm = Vm::new();
            let script = build_fib_script(&mut vm, n);
            assert_eq!(vm.interpret(script), InterpretResult::Ok);
            assert_eq!(vm.output(), [expected.to_string()], "fib({n})");
        }
    }

    #[test]
    fn sum_1_to_100_via_while_loop() {
        let mut vm = Vm::new();
        let script = build_sum_loop(&mut vm, 100);
        let result = vm.interpret(script);

        assert_eq!(result, InterpretResult::Ok);
        assert_eq!(vm.output(), ["5050".to_string()]);
    }

    #[test]
    fn sum_loop_matches_gauss_formula() {
        for n in [0, 1, 5, 37] {
            let mut vm = Vm::new();
            let script = build_sum_loop(&mut vm, n);
            assert_eq!(vm.interpret(script), InterpretResult::Ok);
            let expected = (n * (n + 1) / 2).to_string();
            assert_eq!(vm.output(), [expected], "sum 1..{n}");
        }
    }

    #[test]
    fn gc_demo_completes_and_reclaims_garbage() {
        let mut vm = Vm::new();
        let before = vm.bytes_allocated();

        // Same program as the demo, just fewer iterations so the test stays
        // fast; the GC-stress shape (many throwaway string concats) is the
        // same either way.
        let script = build_gc_demo(&mut vm, 20_000);
        let result = vm.interpret(script);

        assert_eq!(result, InterpretResult::Ok);
        assert_eq!(vm.output(), ["gc demo done".to_string()]);

        let after_run = vm.bytes_allocated();
        vm.collect_garbage();
        let after_gc = vm.bytes_allocated();

        // The collector should never leave *more* live than it found, and a
        // final sweep after 20,000 throwaway concatenations should settle
        // back down near baseline rather than keep growing unbounded.
        assert!(after_gc <= after_run);
        assert!(
            after_gc < before + 4096,
            "expected GC to reclaim the throwaway strings, but {after_gc} bytes are \
             still live (baseline was {before})"
        );
    }

    #[test]
    fn gc_demo_full_scale_matches_original_program() {
        // The exact program from main(): 200,000 iterations.
        let mut vm = Vm::new();
        let script = build_gc_demo(&mut vm, 200_000);
        assert_eq!(vm.interpret(script), InterpretResult::Ok);
        assert_eq!(vm.output(), ["gc demo done".to_string()]);
    }

    // -----------------------------------------------------------------
    // Bonus regression test: this is the interning/rooting scenario we
    // flagged in review — `track_object()` runs the GC-threshold check
    // *after* a string has already been popped off the protecting stack
    // push inside `copy_string`. If a collection fires in that window,
    // the string can get swept (and pruned from the intern table) before
    // it's ever registered as reachable.
    //
    // This only reproduces reliably with a collection forced on *every*
    // allocation, so it's gated behind the `gc_stress` feature:
    //     cargo test --features gc_stress interning_survives
    // -----------------------------------------------------------------
    #[cfg(feature = "gc_stress")]
    #[test]
    fn interning_survives_a_gc_triggered_mid_allocation() {
        let mut vm = Vm::new();

        let a = copy_string(&mut vm, "same-content");
        let b = copy_string(&mut vm, "same-content");

        // Two calls with identical content must yield the *same* interned
        // object. If a GC fired between the intern-table insert and
        // track_object() finishing, `a` could have been pruned from the
        // strings table already, and `b` would allocate a distinct object.
        assert!(
            std::rc::Rc::ptr_eq(&a, &b),
            "expected copy_string to return the same interned Rc twice for identical \
             content, got two different allocations — a GC likely ran between interning \
             and tracking the object"
        );
    }
}

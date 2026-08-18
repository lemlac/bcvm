# bcvm — a small bytecode engine in Rust

A stack-based virtual machine in the same architectural family as the JVM or
the CLR: a compact instruction set operating over an explicit value stack,
per-call stack frames, a runtime constant pool, a managed heap with a real
mark-sweep garbage collector, and a disassembler for inspecting compiled
bytecode.

* Compact instruction set over an explicit value stack
* Per-call stack frames with a classic “slot 0 = callee” calling convention
* Runtime constant pool per function
* Managed heap with mark-sweep GC
* String interning via an open-addressing hash table
* Disassembler for compiled bytecode

There is no source-language parser — like `javap` / `ildasm` territory, this is
the *engine* layer that a compiler would target. `main.rs` builds bytecode
programs directly through the assembler API in `chunk.rs`, which is exactly what
a front-end would call.

## Build & run

```bash
cargo build --release
./target/release/bcvm

# or with per-instruction tracing
cargo run -- --trace

# optional features
cargo run --features gc_log      # log every GC cycle
cargo run --features gc_stress   # collect on every allocation
cargo run --features trace       # compile-time tracing flag (also usable with --trace)
```

## Architecture

| Module  | Role |
|---------|------|
| `value` | Tagged `Value` (nil / bool / number / obj) and heap objects (strings, functions, natives) |
| `chunk` | A `Chunk` = bytecode array + line table + constant pool; opcode enum; assembler API (`emit_*`) |
| `table` | Open-addressing hash table (globals + string interning) |
| `vm`    | Engine: value stack, call-frame stack, interpreter loop, mark-sweep GC |
| `debug` | Disassembler (`disassemble_chunk` / `disassemble_instruction`) |
| `main`  | Demo programs (recursive Fibonacci, while-loop sum, GC stress test) |

### Values and objects

`Value` is a small tagged enum (numbers are `f64`, booleans, nil, or a reference
to a heap object). Heap objects — strings, function objects, native-function
wrappers — share a common `Obj` representation carrying a type tag and a GC
mark bit so the collector can walk the heap uniformly.

### Bytecode and the constant pool

Each function compiles to its own `Chunk`: a flat byte array of opcodes and
operands, a parallel line-number table (for error messages), and a **constant
pool** (literals: numbers, interned strings, nested function objects).
`OP_CONST <idx>` pushes `chunk.constants[idx]`.

### Call frames and calling convention

`vm.frames` is the call stack. Each frame holds the callee, an instruction
pointer into its chunk, and a base index into the shared value stack marking
where its locals begin. Slot 0 of every frame is reserved for the callee
itself (so `OP_GET_LOCAL 1` is a function’s first parameter). `OP_CALL` pushes
a new frame; `OP_RETURN` pops it and splices the result back onto the caller’s
stack.

### Control flow

No unrestricted goto: `OP_JUMP`, `OP_JUMP_IF_FALSE`, and `OP_LOOP` carry 16-bit
relative offsets patched in a second pass (`patch_jump`), which is how
`if` / `while` / `for` desugar in stack VMs of this shape.

### Garbage collection

The VM implements mark-sweep collection:

* **Allocator** – every heap object is tracked; a growing bytes-allocated
  threshold triggers `collect_garbage`.
* **Roots** – the value stack, open call frames’ function objects, and the
  globals table. Function constant pools are traced when their owning
  function is marked.
* **Trace** – a gray-stack worklist marks reachable objects and follows
  references out of function objects into their constant pools.
* **Sweep** – unmarked objects are removed from the heap list; unmarked
  entries are pruned from the string-intern table so it never holds
  dangling references.
* **String interning** – equal string contents share one instance, so
  equality is a pointer compare and temporary strings are exactly what the
  GC demo allocates and reclaims.

### Native functions

`define_native` installs a host function under a global name (`clock` is wired
up at startup), the same escape hatch managed runtimes provide for foreign
code.

## Extending it

* Add an opcode in `chunk::OpCode`, a case in `vm::Vm::run`, and a printer in
  `debug`.
* Add object kinds by extending `Obj`, giving them an allocator in `value`, a
  case in the mark/sweep paths, and printing support.
* The natural next step toward a full language is a recursive-descent
  compiler (lexer → parser → emitter) that calls the `emit_*` helpers already
  in `chunk`, turning this from a bytecode engine into a complete language
  implementation.

## License

MIT

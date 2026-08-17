# bcvm — a small bytecode engine in Rust

A faithful Rust port of [lemlac/bcvm](https://github.com/lemlac/bcvm): a stack-based
virtual machine written in the same architectural family as the JVM or the CLR.

* Compact instruction set operating over an explicit value stack
* Per-call stack frames with a classic “slot 0 = callee” calling convention
* Runtime constant pool per function
* Managed heap with a real mark-sweep garbage collector
* String interning (open-addressing hash table)
* Disassembler for inspecting compiled bytecode

There is no source-language parser — like `javap` / `ildasm` territory, this is
the *engine* layer that a compiler would target.  `main.rs` builds bytecode
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

| Module       | Role |
|--------------|------|
| `value`      | Tagged `Value` (nil/bool/number/obj) and heap objects (`Obj` header + strings, functions, natives) |
| `chunk`      | A `Chunk` = bytecode array + line table + constant pool; opcode enum; assembler API (`emit_*`) |
| `table`      | Open-addressing hash table (globals + string interning) |
| `vm`         | Engine: value stack, call-frame stack, interpreter loop, mark-sweep GC |
| `debug`      | Disassembler (`disassemble_chunk` / `disassemble_instruction`) |
| `main`       | Three demo programs (recursive Fibonacci, while-loop sum, GC stress test) |

### Notable design choices vs. the C original

* **Ownership / GC** – Objects are `Rc<RefCell<Obj>>`.  The mark-sweep collector
  still exists and is triggered by a bytes-allocated threshold; unmarked
  objects are dropped from the VM’s object list (and the `Rc`s go away when
  no other roots remain).  This keeps the educational “real GC” flavour while
  staying memory-safe.
* **Chunks as permanent roots** – Every chunk’s constant pool is registered
  with the VM so its constants stay reachable for the lifetime of the process
  (mirrors the C intrusive list of chunks).
* **No `unsafe` in the interpreter hot path** – The only `unsafe` is a short
  lifetime extension when walking the permanent chunk list during GC; the
  bytecode itself is trusted (produced by our own emitter).

## Extending it

* Add an opcode in `chunk::OpCode`, a case in `vm::Vm::run`, and a printer in
  `debug`.
* Add object kinds by extending `Obj`, giving them an allocator in `value`,
  a case in the mark/sweep paths, and printing support.
* The natural next step toward a full language is a recursive-descent
  compiler that calls the same `emit_*` helpers already present in `chunk`.

## License

MIT (same as the original C implementation).

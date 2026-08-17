# bcvm — a small bytecode engine in C

A stack-based virtual machine written in portable C11, architecturally in
the same family as the JVM or the CLR: a compact instruction set operating
over an explicit value stack, per-call stack frames, a runtime constant
pool, a managed heap with a real mark-sweep garbage collector, and a
disassembler for inspecting compiled bytecode.

There's no source-language parser here — like `javap`/`ildasm` territory,
this is the *engine* layer that a compiler (`javac`, `csc`, ...) would
target. `main.c` builds bytecode programs directly through the assembler
API in `chunk.h`, which is exactly what a front end would call.

## Build & run

```
make            # builds ./bcvm
./bcvm          # runs three demo programs
./bcvm --trace  # also disassembles each program before running it

make trace      # rebuild with per-instruction execution tracing baked in
./bcvm --trace  # now also prints the stack + current instruction every step
```

## Architecture

| File | Role |
|---|---|
| `value.h/.c` | Tagged `Value` (nil/bool/number/obj) and heap objects (`Obj` header + strings, functions, natives) |
| `chunk.h/.c` | A `Chunk` = bytecode array + line table + constant pool; opcode enum; an "assembler" API (`emitByte`, `emitJump`, `patchJump`, `emitLoop`, ...) |
| `table.h/.c` | Open-addressing hash table, used for globals and for string interning |
| `vm.h/.c` | The engine: value stack, call-frame stack, calling convention, the bytecode interpreter loop, and the garbage collector |
| `debug.h/.c` | A disassembler (`disassembleChunk`) that prints a human-readable bytecode listing |
| `main.c` | Demo programs (recursive Fibonacci, a while-loop, a GC stress test) |

### Values and objects

`Value` is a small tagged union (numbers are `double`, booleans, nil, or a
pointer to a heap `Obj`). Every heap object — strings, function objects,
native-function wrappers — shares an `Obj` header carrying a type tag, a
GC mark bit, and an intrusive next-pointer, the same trick both real
runtimes use to walk the whole heap during collection.

### Bytecode & the constant pool

Each function compiles to its own `Chunk`: a flat `uint8_t` array of
opcodes/operands, a parallel line-number table (for error messages), and a
**constant pool** (its literals: numbers boxed as `Value`, interned
strings, and even nested function objects). `OP_CONST <idx>` pushes
`chunk->constants[idx]`, mirroring `ldc` in JVM bytecode or `ldstr`/`ldc.i4`
in CIL.

### Call frames & calling convention

`vm.frames[]` is the call stack. Each `CallFrame` holds the callee, an
instruction pointer into its chunk, and a `slots` pointer into the shared
value stack marking where its locals begin. Slot 0 of every frame is
reserved for the callee itself (so `OP_GET_LOCAL 1` is a function's first
parameter) — the same "receiver slot" idea CLR/JVM calling conventions
use conceptually for `this`. `OP_CALL` pushes a new frame; `OP_RETURN`
pops it and splices the result back onto the caller's stack.

### Control flow

No `goto`-equivalent bytecode: `OP_JUMP`, `OP_JUMP_IF_FALSE`, and
`OP_LOOP` carry 16-bit relative offsets patched in a second pass
(`patchJump`), which is how `if`/`while`/`for` desugar in every stack VM
of this shape.

### Garbage collection

`vm.c` implements real mark-sweep, not just an allocate-and-leak arena:

- **Allocator**: every byte the engine allocates flows through
  `reallocate()` in `memory.c`, which tracks `vm.bytesAllocated` and
  triggers `collectGarbage()` once a growing threshold is crossed.
- **Roots**: the value stack, every open call frame's function, the
  globals table, and — deliberately — the constant pool of *every chunk
  ever compiled* (chunks live for the process lifetime, so their
  constants are always reachable, the same way a JVM's runtime constant
  pool entries are effectively permanent).
- **Trace**: a gray-stack worklist marks reachable objects and follows
  references out of function objects into their own constant pools.
- **Sweep**: unmarked objects are unlinked from the heap list and freed;
  unmarked entries are also pruned from the string-intern table so it
  never holds dangling pointers.
- **String interning**: all string objects with equal content are the
  same instance, so `==`-style equality on the VM's own string comparison
  is a pointer compare, and popped/garbage strings are exactly what the
  GC demo generates and reclaims.

### Native functions

`defineNative()` installs a C function under a global name (a `clock()`
built-in is wired up in `initVM`), the same escape hatch every managed
runtime provides (JNI, P/Invoke, ...).

## Extending it

- Add opcodes in `chunk.h`'s `OpCode` enum, a case in `vm.c`'s `run()`
  loop, and a printer in `debug.c`.
- Add object kinds by extending `ObjType`, giving them an allocator in
  `value.c`, a `case` in `blackenObject`/`freeObject`/`sweep` for GC, and
  printing support in `printValue`.
- The natural next step toward a full language is a recursive-descent
  compiler (lexer → Pratt-parser → emitter) that calls straight into the
  `emit*` functions already in `chunk.h`, turning this from a "bytecode
  engine" into a complete language implementation.

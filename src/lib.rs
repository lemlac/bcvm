//! bcvm – a small stack-based bytecode virtual machine.
//!
//! Architecturally in the same family as the JVM / CLR: a compact instruction
//! set operating over an explicit value stack, per-call stack frames, a
//! runtime constant pool, a managed heap with a real mark-sweep garbage
//! collector, and a disassembler.

pub mod chunk;
pub mod common;
pub mod debug;
pub mod table;
pub mod value;
pub mod vm;

pub use chunk::{Chunk, OpCode};
pub use value::{Obj, ObjFunction, ObjRef, Value};
pub use vm::{InterpretResult, Vm};

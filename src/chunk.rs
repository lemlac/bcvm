//! Bytecode chunks, the constant pool, and a small assembler API.
//!
//! A `Chunk` is a flat array of opcodes + operands, a parallel line-number
//! table, and a constant pool.  The emit_* helpers are what a real compiler
//! front-end would call.

use crate::value::{Value, ValueArray};

/// Opcode set (identical to the C original).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    Const = 0,        // operand: 1-byte constant index
    Nil,
    True,
    False,
    Pop,
    Dup,

    GetLocal,         // operand: 1-byte frame-relative slot
    SetLocal,
    GetGlobal,        // operand: 1-byte constant index → name
    DefineGlobal,
    SetGlobal,

    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Negate,
    Not,
    Equal,
    Greater,
    Less,

    Print,

    Jump,             // operand: 2-byte forward offset
    JumpIfFalse,      // operand: 2-byte forward offset
    Loop,             // operand: 2-byte backward offset

    Call,             // operand: 1-byte arg count
    Return,

    Halt,
}

impl From<u8> for OpCode {
    fn from(b: u8) -> Self {
        // Safety: the bytecode is trusted (produced by our own emitter).
        // A production VM would validate.
        match b {
            0 => OpCode::Const,
            1 => OpCode::Nil,
            2 => OpCode::True,
            3 => OpCode::False,
            4 => OpCode::Pop,
            5 => OpCode::Dup,
            6 => OpCode::GetLocal,
            7 => OpCode::SetLocal,
            8 => OpCode::GetGlobal,
            9 => OpCode::DefineGlobal,
            10 => OpCode::SetGlobal,
            11 => OpCode::Add,
            12 => OpCode::Sub,
            13 => OpCode::Mul,
            14 => OpCode::Div,
            15 => OpCode::Mod,
            16 => OpCode::Negate,
            17 => OpCode::Not,
            18 => OpCode::Equal,
            19 => OpCode::Greater,
            20 => OpCode::Less,
            21 => OpCode::Print,
            22 => OpCode::Jump,
            23 => OpCode::JumpIfFalse,
            24 => OpCode::Loop,
            25 => OpCode::Call,
            26 => OpCode::Return,
            27 => OpCode::Halt,
            _ => panic!("unknown opcode {}", b),
        }
    }
}

impl From<OpCode> for u8 {
    fn from(op: OpCode) -> u8 {
        op as u8
    }
}

/// A compiled function body.
#[derive(Debug)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub lines: Vec<i32>,
    pub constants: ValueArray,
    /// Used by the VM to keep an intrusive list of every chunk (permanent roots).
    pub(crate) registered: bool,
}

impl Chunk {
    pub fn new() -> Self {
        Chunk {
            code: Vec::new(),
            lines: Vec::new(),
            constants: ValueArray::new(),
            registered: false,
        }
    }

    pub fn write(&mut self, byte: u8, line: i32) {
        self.code.push(byte);
        self.lines.push(line);
    }

    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.write(value)
    }

    // ------------------------------------------------------------------
    // Assembler helpers (what a front-end would call)
    // ------------------------------------------------------------------

    pub fn emit_byte(&mut self, byte: u8, line: i32) {
        self.write(byte, line);
    }

    pub fn emit_bytes(&mut self, b1: u8, b2: u8, line: i32) {
        self.write(b1, line);
        self.write(b2, line);
    }

    pub fn emit_op(&mut self, op: OpCode, line: i32) {
        self.write(op as u8, line);
    }

    /// Emit `OP_CONST <idx>` and return the constant index.
    pub fn emit_constant(&mut self, value: Value, line: i32) -> usize {
        let index = self.add_constant(value);
        if index > 255 {
            panic!("too many constants in one chunk");
        }
        self.emit_bytes(OpCode::Const as u8, index as u8, line);
        index
    }

    /// Emit a jump instruction with a placeholder offset; returns the offset
    /// that later needs to be patched.
    pub fn emit_jump(&mut self, instruction: OpCode, line: i32) -> usize {
        self.emit_byte(instruction as u8, line);
        self.emit_byte(0xff, line);
        self.emit_byte(0xff, line);
        self.code.len() - 2
    }

    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.code.len() - offset - 2;
        if jump > u16::MAX as usize {
            panic!("jump too large");
        }
        self.code[offset] = ((jump >> 8) & 0xff) as u8;
        self.code[offset + 1] = (jump & 0xff) as u8;
    }

    pub fn emit_loop(&mut self, loop_start: usize, line: i32) {
        self.emit_byte(OpCode::Loop as u8, line);
        let offset = self.code.len() - loop_start + 2;
        if offset > u16::MAX as usize {
            panic!("loop body too large");
        }
        self.emit_byte(((offset >> 8) & 0xff) as u8, line);
        self.emit_byte((offset & 0xff) as u8, line);
    }

    pub fn count(&self) -> usize {
        self.code.len()
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

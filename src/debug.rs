//! Disassembler – prints a human-readable listing of a Chunk.

use crate::chunk::{Chunk, OpCode};

fn simple_instruction(name: &str, offset: usize) -> usize {
    println!("{}", name);
    offset + 1
}

fn byte_instruction(name: &str, chunk: &Chunk, offset: usize) -> usize {
    let slot = chunk.code[offset + 1];
    println!("{:<16} {:4}", name, slot);
    offset + 2
}

fn constant_instruction(name: &str, chunk: &Chunk, offset: usize) -> usize {
    let index = chunk.code[offset + 1] as usize;
    print!("{:<16} {:4} '", name, index);
    if index < chunk.constants.values.len() {
        print!("{}", chunk.constants.values[index]);
    } else {
        print!("?");
    }
    println!("'");
    offset + 2
}

fn jump_instruction(name: &str, sign: i32, chunk: &Chunk, offset: usize) -> usize {
    let jump = ((chunk.code[offset + 1] as u16) << 8) | (chunk.code[offset + 2] as u16);
    println!(
        "{:<16} {:4} -> {}",
        name,
        offset,
        offset as i32 + 3 + sign * jump as i32
    );
    offset + 3
}

pub fn disassemble_instruction(chunk: &Chunk, offset: usize) -> usize {
    print!("{:04} ", offset);
    if offset > 0 && chunk.lines[offset] == chunk.lines[offset - 1] {
        print!("   | ");
    } else {
        print!("{:4} ", chunk.lines[offset]);
    }

    let instruction = OpCode::from(chunk.code[offset]);
    match instruction {
        OpCode::Const => constant_instruction("OP_CONST", chunk, offset),
        OpCode::Nil => simple_instruction("OP_NIL", offset),
        OpCode::True => simple_instruction("OP_TRUE", offset),
        OpCode::False => simple_instruction("OP_FALSE", offset),
        OpCode::Pop => simple_instruction("OP_POP", offset),
        OpCode::Dup => simple_instruction("OP_DUP", offset),
        OpCode::GetLocal => byte_instruction("OP_GET_LOCAL", chunk, offset),
        OpCode::SetLocal => byte_instruction("OP_SET_LOCAL", chunk, offset),
        OpCode::GetGlobal => constant_instruction("OP_GET_GLOBAL", chunk, offset),
        OpCode::DefineGlobal => constant_instruction("OP_DEFINE_GLOBAL", chunk, offset),
        OpCode::SetGlobal => constant_instruction("OP_SET_GLOBAL", chunk, offset),
        OpCode::Add => simple_instruction("OP_ADD", offset),
        OpCode::Sub => simple_instruction("OP_SUB", offset),
        OpCode::Mul => simple_instruction("OP_MUL", offset),
        OpCode::Div => simple_instruction("OP_DIV", offset),
        OpCode::Mod => simple_instruction("OP_MOD", offset),
        OpCode::Negate => simple_instruction("OP_NEGATE", offset),
        OpCode::Not => simple_instruction("OP_NOT", offset),
        OpCode::Equal => simple_instruction("OP_EQUAL", offset),
        OpCode::Greater => simple_instruction("OP_GREATER", offset),
        OpCode::Less => simple_instruction("OP_LESS", offset),
        OpCode::Print => simple_instruction("OP_PRINT", offset),
        OpCode::Jump => jump_instruction("OP_JUMP", 1, chunk, offset),
        OpCode::JumpIfFalse => jump_instruction("OP_JUMP_IF_FALSE", 1, chunk, offset),
        OpCode::Loop => jump_instruction("OP_LOOP", -1, chunk, offset),
        OpCode::Call => byte_instruction("OP_CALL", chunk, offset),
        OpCode::Return => simple_instruction("OP_RETURN", offset),
        OpCode::Halt => simple_instruction("OP_HALT", offset),
    }
}

pub fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut offset = 0;
    while offset < chunk.code.len() {
        offset = disassemble_instruction(chunk, offset);
    }
}

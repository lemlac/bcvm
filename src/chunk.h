#ifndef BCVM_CHUNK_H
#define BCVM_CHUNK_H

#include "common.h"
#include "value.h"

typedef enum {
    OP_CONST,        /* operand: 1-byte constant index -> push constants[i]      */
    OP_NIL,
    OP_TRUE,
    OP_FALSE,
    OP_POP,
    OP_DUP,

    OP_GET_LOCAL,    /* operand: 1-byte frame-relative slot                      */
    OP_SET_LOCAL,
    OP_GET_GLOBAL,   /* operand: 1-byte constant index -> name                   */
    OP_DEFINE_GLOBAL,
    OP_SET_GLOBAL,

    OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MOD,
    OP_NEGATE, OP_NOT,
    OP_EQUAL, OP_GREATER, OP_LESS,

    OP_PRINT,

    OP_JUMP,            /* operand: 2-byte forward offset                       */
    OP_JUMP_IF_FALSE,   /* operand: 2-byte forward offset                       */
    OP_LOOP,            /* operand: 2-byte backward offset                      */

    OP_CALL,            /* operand: 1-byte arg count                            */
    OP_RETURN,

    OP_HALT
} OpCode;

struct Chunk {
    int count;
    int capacity;
    uint8_t* code;
    int* lines;
    ValueArray constants;
    struct Chunk* nextChunk; /* links every chunk ever created, see vm.c registry */
};

Chunk* newChunk(void);              /* allocates + registers a permanent chunk */
void initChunk(Chunk* chunk);       /* in-place init, also registers it        */
void freeChunk(Chunk* chunk);       /* frees only the internal arrays          */
void writeChunk(Chunk* chunk, uint8_t byte, int line);
int addConstant(Chunk* chunk, Value value); /* returns index, dedups nothing */

/* Convenience emitters used by hand-written "assembly" programs. */
void emitByte(Chunk* chunk, uint8_t byte, int line);
void emitBytes(Chunk* chunk, uint8_t b1, uint8_t b2, int line);
int emitConstant(Chunk* chunk, Value value, int line);      /* OP_CONST idx  */
int emitJump(Chunk* chunk, uint8_t instruction, int line);  /* returns patch offset */
void patchJump(Chunk* chunk, int offset);
void emitLoop(Chunk* chunk, int loopStart, int line);

#endif

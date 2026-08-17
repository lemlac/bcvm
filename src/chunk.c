#include <stdio.h>
#include <stdlib.h>

#include "chunk.h"
#include "memory.h"
#include "vm.h"

Chunk* newChunk(void) {
    Chunk* chunk = (Chunk*)malloc(sizeof(Chunk));
    if (!chunk) { fprintf(stderr, "bcvm: out of memory\n"); exit(1); }
    initChunk(chunk);
    return chunk;
}

void initChunk(Chunk* chunk) {
    chunk->count = 0;
    chunk->capacity = 0;
    chunk->code = NULL;
    chunk->lines = NULL;
    chunk->nextChunk = NULL;
    initValueArray(&chunk->constants);
    registerChunk(chunk); /* constant pool objects become permanent GC roots */
}

void freeChunk(Chunk* chunk) {
    FREE_ARRAY(uint8_t, chunk->code, chunk->capacity);
    FREE_ARRAY(int, chunk->lines, chunk->capacity);
    freeValueArray(&chunk->constants);
    chunk->code = NULL;
    chunk->lines = NULL;
    chunk->capacity = chunk->count = 0;
}

void writeChunk(Chunk* chunk, uint8_t byte, int line) {
    if (chunk->capacity < chunk->count + 1) {
        int oldCap = chunk->capacity;
        chunk->capacity = GROW_CAPACITY(oldCap);
        chunk->code = GROW_ARRAY(uint8_t, chunk->code, oldCap, chunk->capacity);
        chunk->lines = GROW_ARRAY(int, chunk->lines, oldCap, chunk->capacity);
    }
    chunk->code[chunk->count] = byte;
    chunk->lines[chunk->count] = line;
    chunk->count++;
}

int addConstant(Chunk* chunk, Value value) {
    push(value); /* keep reachable while the array grows (GC safety) */
    int index = writeValueArray(&chunk->constants, value);
    pop();
    return index;
}

void emitByte(Chunk* chunk, uint8_t byte, int line) {
    writeChunk(chunk, byte, line);
}

void emitBytes(Chunk* chunk, uint8_t b1, uint8_t b2, int line) {
    writeChunk(chunk, b1, line);
    writeChunk(chunk, b2, line);
}

int emitConstant(Chunk* chunk, Value value, int line) {
    int index = addConstant(chunk, value);
    if (index > 255) {
        fprintf(stderr, "too many constants in one chunk\n");
        exit(1);
    }
    emitBytes(chunk, OP_CONST, (uint8_t)index, line);
    return index;
}

int emitJump(Chunk* chunk, uint8_t instruction, int line) {
    emitByte(chunk, instruction, line);
    emitByte(chunk, 0xff, line);
    emitByte(chunk, 0xff, line);
    return chunk->count - 2;
}

void patchJump(Chunk* chunk, int offset) {
    int jump = chunk->count - offset - 2;
    if (jump > UINT16_MAX) {
        fprintf(stderr, "jump too large\n");
        exit(1);
    }
    chunk->code[offset] = (jump >> 8) & 0xff;
    chunk->code[offset + 1] = jump & 0xff;
}

void emitLoop(Chunk* chunk, int loopStart, int line) {
    emitByte(chunk, OP_LOOP, line);
    int offset = chunk->count - loopStart + 2;
    if (offset > UINT16_MAX) {
        fprintf(stderr, "loop body too large\n");
        exit(1);
    }
    emitByte(chunk, (offset >> 8) & 0xff, line);
    emitByte(chunk, offset & 0xff, line);
}

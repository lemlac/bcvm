#ifndef BCVM_VM_H
#define BCVM_VM_H

#include "chunk.h"
#include "table.h"
#include "value.h"

#define FRAMES_MAX 256
#define STACK_MAX  (FRAMES_MAX * 64)

typedef struct {
    ObjFunction* function;
    uint8_t* ip;      /* instruction pointer into function->chunk->code */
    Value* slots;      /* base of this frame's locals within vm.stack   */
} CallFrame;

typedef struct {
    CallFrame frames[FRAMES_MAX];
    int frameCount;

    Value stack[STACK_MAX];
    Value* stackTop;

    Table globals;
    Table strings;      /* interned string pool */

    Obj* objects;       /* intrusive list of every live heap object */
    Chunk* allChunks;    /* intrusive list of every chunk (permanent roots) */

    size_t bytesAllocated;
    size_t nextGC;

    /* GC worklist (gray set), a plain growable array of Obj* */
    int grayCount;
    int grayCapacity;
    Obj** grayStack;
} VM;

typedef enum {
    INTERPRET_OK,
    INTERPRET_COMPILE_ERROR,
    INTERPRET_RUNTIME_ERROR
} InterpretResult;

extern VM vm;

void initVM(void);
void freeVM(void);

/* Run `entry`, a zero-arg top-level function, to completion. */
InterpretResult interpret(ObjFunction* entry);

void push(Value value);
Value pop(void);
Value peek(int distanceFromTop);

void defineNative(const char* name, NativeFn fn);
void registerChunk(Chunk* chunk); /* called by initChunk() */

/* GC entry points, used by memory.c / table.c / chunk.c */
void collectGarbage(void);
void markValue(Value value);
void markObject(Obj* object);

#endif

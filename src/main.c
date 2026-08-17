#include <stdio.h>
#include <string.h>

#include "chunk.h"
#include "debug.h"
#include "memory.h"
#include "value.h"
#include "vm.h"

/* This file plays the role that a compiler front-end (javac, csc, ...)
 * would normally play: it emits bytecode into Chunks. Here we do it by
 * hand, in C, which also doubles as a demonstration of the engine's
 * "assembly-level" API. A real front end would parse source text and
 * call the same emit* functions found in chunk.h. */

static ObjString* name(const char* s) { return copyString(s, (int)strlen(s)); }

/* ---------------------------------------------------------------------
 * Program 1: recursive Fibonacci, exercising function calls, locals,
 * conditionals and recursion.
 *
 *   fn fib(n) {
 *       if (n < 2) return n;
 *       return fib(n - 1) + fib(n - 2);
 *   }
 *   print fib(21);
 * ------------------------------------------------------------------- */
static ObjFunction* buildFib(void) {
    ObjFunction* fn = newFunction();
    fn->arity = 1;
    fn->name = name("fib");
    Chunk* c = fn->chunk;

    emitBytes(c, OP_GET_LOCAL, 1, 1);          /* n (slot 0 is the callee itself) */
    emitConstant(c, NUMBER_VAL(2), 1);          /* 2            */
    emitByte(c, OP_LESS, 1);                    /* n < 2        */
    int thenJump = emitJump(c, OP_JUMP_IF_FALSE, 1);
    emitByte(c, OP_POP, 1);
    emitBytes(c, OP_GET_LOCAL, 1, 1);
    emitByte(c, OP_RETURN, 1);
    int elseJump = emitJump(c, OP_JUMP, 1);
    patchJump(c, thenJump);
    emitByte(c, OP_POP, 1);

    int fibNameIdx = addConstant(c, OBJ_VAL(name("fib")));
    emitBytes(c, OP_GET_GLOBAL, (uint8_t)fibNameIdx, 2);
    emitBytes(c, OP_GET_LOCAL, 1, 2);
    emitConstant(c, NUMBER_VAL(1), 2);
    emitByte(c, OP_SUB, 2);
    emitBytes(c, OP_CALL, 1, 2);

    emitBytes(c, OP_GET_GLOBAL, (uint8_t)fibNameIdx, 2);
    emitBytes(c, OP_GET_LOCAL, 1, 2);
    emitConstant(c, NUMBER_VAL(2), 2);
    emitByte(c, OP_SUB, 2);
    emitBytes(c, OP_CALL, 1, 2);

    emitByte(c, OP_ADD, 2);
    emitByte(c, OP_RETURN, 2);

    patchJump(c, elseJump);
    emitByte(c, OP_NIL, 3);
    emitByte(c, OP_RETURN, 3);
    return fn;
}

static ObjFunction* buildFibScript(int arg) {
    ObjFunction* script = newFunction();
    script->name = NULL;
    Chunk* c = script->chunk;

    emitConstant(c, OBJ_VAL(buildFib()), 1);
    int fibNameIdx = addConstant(c, OBJ_VAL(name("fib")));
    emitBytes(c, OP_DEFINE_GLOBAL, (uint8_t)fibNameIdx, 1);

    emitBytes(c, OP_GET_GLOBAL, (uint8_t)fibNameIdx, 2);
    emitConstant(c, NUMBER_VAL(arg), 2);
    emitBytes(c, OP_CALL, 1, 2);
    emitByte(c, OP_PRINT, 2);

    emitByte(c, OP_NIL, 3);
    emitByte(c, OP_RETURN, 3);
    return script;
}

/* ---------------------------------------------------------------------
 * Program 2: iterative loop over locals, exercising OP_LOOP / jumps.
 *
 *   var i = 1; var sum = 0;
 *   while (i <= n) { sum = sum + i; i = i + 1; }
 *   print sum;
 * ------------------------------------------------------------------- */
static ObjFunction* buildSumLoop(int n) {
    ObjFunction* script = newFunction();
    script->name = NULL;
    Chunk* c = script->chunk;

    /* locals: slot 0 is the script's own function value (reserved by the
       calling convention), slot 1 = i, slot 2 = sum */
    emitConstant(c, NUMBER_VAL(1), 1);  /* i = 1   (slot 1) */
    emitConstant(c, NUMBER_VAL(0), 1);  /* sum = 0 (slot 2) */

    int loopStart = c->count;
    /* condition: (i <= n)  computed as NOT(i > n) since there's no
       dedicated less-equal opcode -- keeps the instruction set small. */
    emitBytes(c, OP_GET_LOCAL, 1, 2);
    emitConstant(c, NUMBER_VAL(n), 2);
    emitByte(c, OP_GREATER, 2);   /* push (i > n) */
    emitByte(c, OP_NOT, 2);       /* push (i <= n) */
    int exitJump = emitJump(c, OP_JUMP_IF_FALSE, 2);
    emitByte(c, OP_POP, 2); /* discard the true condition value */

    /* body: sum = sum + i */
    emitBytes(c, OP_GET_LOCAL, 2, 3);
    emitBytes(c, OP_GET_LOCAL, 1, 3);
    emitByte(c, OP_ADD, 3);
    emitBytes(c, OP_SET_LOCAL, 2, 3);
    emitByte(c, OP_POP, 3);

    /* i = i + 1 */
    emitBytes(c, OP_GET_LOCAL, 1, 4);
    emitConstant(c, NUMBER_VAL(1), 4);
    emitByte(c, OP_ADD, 4);
    emitBytes(c, OP_SET_LOCAL, 1, 4);
    emitByte(c, OP_POP, 4);

    emitLoop(c, loopStart, 4);

    patchJump(c, exitJump);
    emitByte(c, OP_POP, 5); /* discard the false condition value */

    emitBytes(c, OP_GET_LOCAL, 2, 6);
    emitByte(c, OP_PRINT, 6);

    emitByte(c, OP_NIL, 7);
    emitByte(c, OP_RETURN, 7);
    return script;
}

/* ---------------------------------------------------------------------
 * Program 3: string concatenation in a loop, to make garbage and show
 * the collector actually reclaiming it.
 * ------------------------------------------------------------------- */
static ObjFunction* buildGcDemo(int iterations) {
    ObjFunction* script = newFunction();
    script->name = NULL;
    Chunk* c = script->chunk;

    emitConstant(c, NUMBER_VAL(0), 1); /* i (slot 1; slot 0 is the script fn itself) */

    int loopStart = c->count;
    emitBytes(c, OP_GET_LOCAL, 1, 2);
    emitConstant(c, NUMBER_VAL(iterations), 2);
    emitByte(c, OP_LESS, 2);                       /* i < iterations */
    int exit = emitJump(c, OP_JUMP_IF_FALSE, 2);
    emitByte(c, OP_POP, 2);

    /* build a throwaway string "x" each iteration and drop it (garbage) */
    emitConstant(c, OBJ_VAL(name("garbage-")), 3);
    emitConstant(c, OBJ_VAL(name("chunk")), 3);
    emitByte(c, OP_ADD, 3);
    emitByte(c, OP_POP, 3);

    emitBytes(c, OP_GET_LOCAL, 1, 4);
    emitConstant(c, NUMBER_VAL(1), 4);
    emitByte(c, OP_ADD, 4);
    emitBytes(c, OP_SET_LOCAL, 1, 4);
    emitByte(c, OP_POP, 4);

    emitLoop(c, loopStart, 4);
    patchJump(c, exit);
    emitByte(c, OP_POP, 5);

    int msgIdx = addConstant(c, OBJ_VAL(name("gc demo done")));
    emitBytes(c, OP_CONST, (uint8_t)msgIdx, 6);
    emitByte(c, OP_PRINT, 6);

    emitByte(c, OP_NIL, 7);
    emitByte(c, OP_RETURN, 7);
    return script;
}

static void runProgram(const char* title, ObjFunction* script, bool trace) {
    printf("\n===== %s =====\n", title);
    if (trace) disassembleChunk(script->chunk, title);
    printf("-- output --\n");
    InterpretResult result = interpret(script);
    if (result == INTERPRET_RUNTIME_ERROR) {
        printf("(runtime error)\n");
    }
}

int main(int argc, char** argv) {
    bool trace = (argc > 1 && strcmp(argv[1], "--trace") == 0);

    initVM();

    runProgram("fib(21) via recursive calls", buildFibScript(21), trace);
    runProgram("sum 1..100 via a while-loop", buildSumLoop(100), trace);

    printf("\n===== gc demo (200,000 throwaway string concats) =====\n");
    printf("bytesAllocated before:            %8zu\n", vm.bytesAllocated);
    InterpretResult r = interpret(buildGcDemo(200000));
    if (r == INTERPRET_RUNTIME_ERROR) printf("(runtime error)\n");
    printf("bytesAllocated right after loop:   %8zu  (heap-tracking GC already\n"
           "                                            ran automatically mid-loop\n"
           "                                            as soon as the threshold was hit)\n",
           vm.bytesAllocated);
    collectGarbage();
    printf("bytesAllocated after explicit GC:  %8zu\n", vm.bytesAllocated);

    freeVM();
    return 0;
}

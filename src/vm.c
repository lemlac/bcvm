#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "chunk.h"
#include "debug.h"
#include "memory.h"
#include "vm.h"

VM vm;

#define GC_HEAP_GROW_FACTOR 2

/* ---------------------------------------------------------------------
 * Stack helpers
 * ------------------------------------------------------------------- */

void push(Value value) {
    *vm.stackTop = value;
    vm.stackTop++;
}

Value pop(void) {
    vm.stackTop--;
    return *vm.stackTop;
}

Value peek(int distance) {
    return vm.stackTop[-1 - distance];
}

static void resetStack(void) {
    vm.stackTop = vm.stack;
    vm.frameCount = 0;
}

static void runtimeError(const char* format, ...) {
    va_list args;
    va_start(args, format);
    vfprintf(stderr, format, args);
    va_end(args);
    fputc('\n', stderr);

    for (int i = vm.frameCount - 1; i >= 0; i--) {
        CallFrame* frame = &vm.frames[i];
        ObjFunction* fn = frame->function;
        size_t instr = frame->ip - fn->chunk->code - 1;
        int line = fn->chunk->lines[instr];
        fprintf(stderr, "  [line %d] in %s\n", line,
                fn->name ? fn->name->chars : "<script>");
    }
    resetStack();
}

/* ---------------------------------------------------------------------
 * Native functions
 * ------------------------------------------------------------------- */

static Value clockNative(int argCount, Value* args) {
    (void)argCount; (void)args;
    return NUMBER_VAL((double)clock() / CLOCKS_PER_SEC);
}

void defineNative(const char* name, NativeFn function) {
    push(OBJ_VAL(copyString(name, (int)strlen(name))));
    push(OBJ_VAL(newNative(function, name)));
    tableSet(&vm.globals, AS_STRING(vm.stack[0]), vm.stack[1]);
    pop();
    pop();
}

/* ---------------------------------------------------------------------
 * VM lifecycle
 * ------------------------------------------------------------------- */

void initVM(void) {
    resetStack();
    vm.objects = NULL;
    vm.allChunks = NULL;
    vm.bytesAllocated = 0;
    vm.nextGC = 1024 * 1024;
    vm.grayCount = 0;
    vm.grayCapacity = 0;
    vm.grayStack = NULL;

    initTable(&vm.globals);
    initTable(&vm.strings);

    defineNative("clock", clockNative);
}

void freeVM(void) {
    freeTable(&vm.globals);
    freeTable(&vm.strings);
    freeObjects();
}

void registerChunk(Chunk* chunk) {
    chunk->nextChunk = vm.allChunks;
    vm.allChunks = chunk;
}

/* ---------------------------------------------------------------------
 * Garbage collector (mark-sweep). Roots are: the value stack, open call
 * frames' function objects, the globals table, and every constant pool
 * of every chunk ever created (chunks are permanent for the life of the
 * VM, so their constants are always reachable -- mirroring how a JVM's
 * runtime constant pool keeps its entries alive).
 * ------------------------------------------------------------------- */

void markValue(Value value) {
    if (IS_OBJ(value)) markObject(AS_OBJ(value));
}

void markObject(Obj* object) {
    if (object == NULL || object->isMarked) return;
#if GC_LOG
    fprintf(stderr, "%p mark ", (void*)object);
    printValue(OBJ_VAL(object));
    fprintf(stderr, "\n");
#endif
    object->isMarked = true;

    if (vm.grayCapacity < vm.grayCount + 1) {
        vm.grayCapacity = GROW_CAPACITY(vm.grayCapacity);
        vm.grayStack = (Obj**)realloc(vm.grayStack, sizeof(Obj*) * vm.grayCapacity);
        if (vm.grayStack == NULL) { fprintf(stderr, "bcvm: out of memory (gc)\n"); exit(1); }
    }
    vm.grayStack[vm.grayCount++] = object;
}

static void markArray(ValueArray* array) {
    for (int i = 0; i < array->count; i++) markValue(array->values[i]);
}

static void blackenObject(Obj* object) {
    switch (object->type) {
        case OBJ_FUNCTION: {
            ObjFunction* fn = (ObjFunction*)object;
            markObject((Obj*)fn->name);
            markArray(&fn->chunk->constants);
            break;
        }
        case OBJ_NATIVE:
        case OBJ_STRING:
            break; /* no outgoing references */
    }
}

static void markRoots(void) {
    for (Value* slot = vm.stack; slot < vm.stackTop; slot++) markValue(*slot);
    for (int i = 0; i < vm.frameCount; i++) markObject((Obj*)vm.frames[i].function);
    markTable(&vm.globals);

    /* constant pools of every chunk ever compiled are permanent roots */
    for (Chunk* c = vm.allChunks; c != NULL; c = c->nextChunk) {
        markArray(&c->constants);
    }
}

static void traceReferences(void) {
    while (vm.grayCount > 0) {
        Obj* object = vm.grayStack[--vm.grayCount];
        blackenObject(object);
    }
}

static void sweep(void) {
    Obj* previous = NULL;
    Obj* object = vm.objects;
    while (object != NULL) {
        if (object->isMarked) {
            object->isMarked = false;
            previous = object;
            object = object->next;
        } else {
            Obj* unreached = object;
            object = object->next;
            if (previous != NULL) previous->next = object;
            else vm.objects = object;

            switch (unreached->type) {
                case OBJ_STRING: {
                    ObjString* s = (ObjString*)unreached;
                    reallocate(s, sizeof(ObjString) + s->length + 1, 0);
                    break;
                }
                case OBJ_FUNCTION: FREE(ObjFunction, unreached); break;
                case OBJ_NATIVE:   FREE(ObjNative, unreached); break;
            }
        }
    }
}

void collectGarbage(void) {
#if GC_LOG
    fprintf(stderr, "-- gc begin\n");
    size_t before = vm.bytesAllocated;
#endif

    markRoots();
    traceReferences();
    tableRemoveWhite(&vm.strings);
    sweep();

    vm.nextGC = vm.bytesAllocated * GC_HEAP_GROW_FACTOR;

#if GC_LOG
    fprintf(stderr, "-- gc end   collected %zu bytes (%zu -> %zu), next at %zu\n",
            before - vm.bytesAllocated, before, vm.bytesAllocated, vm.nextGC);
#endif
}

/* ---------------------------------------------------------------------
 * Calling convention
 * ------------------------------------------------------------------- */

static bool call(ObjFunction* fn, int argCount) {
    if (argCount != fn->arity) {
        runtimeError("expected %d argument(s) but got %d", fn->arity, argCount);
        return false;
    }
    if (vm.frameCount == FRAMES_MAX) {
        runtimeError("stack overflow");
        return false;
    }
    CallFrame* frame = &vm.frames[vm.frameCount++];
    frame->function = fn;
    frame->ip = fn->chunk->code;
    frame->slots = vm.stackTop - argCount - 1;
    return true;
}

static bool callValue(Value callee, int argCount) {
    if (IS_OBJ(callee)) {
        switch (OBJ_TYPE(callee)) {
            case OBJ_FUNCTION:
                return call(AS_FUNCTION(callee), argCount);
            case OBJ_NATIVE: {
                NativeFn native = AS_NATIVE(callee);
                Value result = native(argCount, vm.stackTop - argCount);
                vm.stackTop -= argCount + 1;
                push(result);
                return true;
            }
            default: break;
        }
    }
    runtimeError("can only call functions (got a %s)", typeName(callee));
    return false;
}

static bool isFalsey(Value v) {
    return IS_NIL(v) || (IS_BOOL(v) && !AS_BOOL(v));
}

static void concatenate(void) {
    ObjString* b = AS_STRING(peek(0));
    ObjString* a = AS_STRING(peek(1));

    int length = a->length + b->length;
    char* chars = ALLOCATE(char, length + 1);
    memcpy(chars, a->chars, a->length);
    memcpy(chars + a->length, b->chars, b->length);
    chars[length] = '\0';

    ObjString* result = takeString(chars, length);
    pop(); pop();
    push(OBJ_VAL(result));
}

/* ---------------------------------------------------------------------
 * The interpreter loop
 * ------------------------------------------------------------------- */

static InterpretResult run(void) {
    CallFrame* frame = &vm.frames[vm.frameCount - 1];

#define READ_BYTE()  (*frame->ip++)
#define READ_SHORT() (frame->ip += 2, (uint16_t)((frame->ip[-2] << 8) | frame->ip[-1]))
#define READ_CONSTANT() (frame->function->chunk->constants.values[READ_BYTE()])
#define READ_STRING() AS_STRING(READ_CONSTANT())

#define BINARY_NUMERIC_OP(valueWrap, op)                                    \
    do {                                                                    \
        if (!IS_NUMBER(peek(0)) || !IS_NUMBER(peek(1))) {                   \
            runtimeError("operands must be numbers");                      \
            return INTERPRET_RUNTIME_ERROR;                                 \
        }                                                                    \
        double b = AS_NUMBER(pop());                                        \
        double a = AS_NUMBER(pop());                                        \
        push(valueWrap(a op b));                                            \
    } while (false)

    for (;;) {
#ifdef BCVM_TRACE
        printf("          ");
        for (Value* slot = vm.stack; slot < vm.stackTop; slot++) {
            printf("[ "); printValue(*slot); printf(" ]");
        }
        printf("\n");
        disassembleInstruction(frame->function->chunk,
                                (int)(frame->ip - frame->function->chunk->code));
#endif
        uint8_t instruction = READ_BYTE();
        switch (instruction) {
            case OP_CONST:  push(READ_CONSTANT()); break;
            case OP_NIL:    push(NIL_VAL); break;
            case OP_TRUE:   push(BOOL_VAL(true)); break;
            case OP_FALSE:  push(BOOL_VAL(false)); break;
            case OP_POP:    pop(); break;
            case OP_DUP:    push(peek(0)); break;

            case OP_GET_LOCAL: push(frame->slots[READ_BYTE()]); break;
            case OP_SET_LOCAL: frame->slots[READ_BYTE()] = peek(0); break;

            case OP_GET_GLOBAL: {
                ObjString* name = READ_STRING();
                Value value;
                if (!tableGet(&vm.globals, name, &value)) {
                    runtimeError("undefined variable '%s'", name->chars);
                    return INTERPRET_RUNTIME_ERROR;
                }
                push(value);
                break;
            }
            case OP_DEFINE_GLOBAL: {
                ObjString* name = READ_STRING();
                tableSet(&vm.globals, name, peek(0));
                pop();
                break;
            }
            case OP_SET_GLOBAL: {
                ObjString* name = READ_STRING();
                if (tableSet(&vm.globals, name, peek(0))) {
                    tableDelete(&vm.globals, name);
                    runtimeError("undefined variable '%s'", name->chars);
                    return INTERPRET_RUNTIME_ERROR;
                }
                break;
            }

            case OP_ADD: {
                if (IS_STRING(peek(0)) && IS_STRING(peek(1))) {
                    concatenate();
                } else if (IS_NUMBER(peek(0)) && IS_NUMBER(peek(1))) {
                    double b = AS_NUMBER(pop());
                    double a = AS_NUMBER(pop());
                    push(NUMBER_VAL(a + b));
                } else {
                    runtimeError("operands must be two numbers or two strings");
                    return INTERPRET_RUNTIME_ERROR;
                }
                break;
            }
            case OP_SUB: BINARY_NUMERIC_OP(NUMBER_VAL, -); break;
            case OP_MUL: BINARY_NUMERIC_OP(NUMBER_VAL, *); break;
            case OP_DIV: BINARY_NUMERIC_OP(NUMBER_VAL, /); break;
            case OP_MOD: {
                if (!IS_NUMBER(peek(0)) || !IS_NUMBER(peek(1))) {
                    runtimeError("operands must be numbers");
                    return INTERPRET_RUNTIME_ERROR;
                }
                double b = AS_NUMBER(pop());
                double a = AS_NUMBER(pop());
                push(NUMBER_VAL(fmod(a, b)));
                break;
            }
            case OP_NEGATE:
                if (!IS_NUMBER(peek(0))) {
                    runtimeError("operand must be a number");
                    return INTERPRET_RUNTIME_ERROR;
                }
                push(NUMBER_VAL(-AS_NUMBER(pop())));
                break;
            case OP_NOT: push(BOOL_VAL(isFalsey(pop()))); break;

            case OP_EQUAL: {
                Value b = pop(), a = pop();
                push(BOOL_VAL(valuesEqual(a, b)));
                break;
            }
            case OP_GREATER: BINARY_NUMERIC_OP(BOOL_VAL, >); break;
            case OP_LESS:    BINARY_NUMERIC_OP(BOOL_VAL, <); break;

            case OP_PRINT: printValue(pop()); printf("\n"); break;

            case OP_JUMP: {
                uint16_t offset = READ_SHORT();
                frame->ip += offset;
                break;
            }
            case OP_JUMP_IF_FALSE: {
                uint16_t offset = READ_SHORT();
                if (isFalsey(peek(0))) frame->ip += offset;
                break;
            }
            case OP_LOOP: {
                uint16_t offset = READ_SHORT();
                frame->ip -= offset;
                break;
            }

            case OP_CALL: {
                int argCount = READ_BYTE();
                if (!callValue(peek(argCount), argCount)) {
                    return INTERPRET_RUNTIME_ERROR;
                }
                frame = &vm.frames[vm.frameCount - 1];
                break;
            }
            case OP_RETURN: {
                Value result = pop();
                vm.frameCount--;
                if (vm.frameCount == 0) {
                    pop(); /* the top-level script "function" value */
                    return INTERPRET_OK;
                }
                vm.stackTop = frame->slots;
                push(result);
                frame = &vm.frames[vm.frameCount - 1];
                break;
            }

            case OP_HALT:
                return INTERPRET_OK;
        }
    }

#undef READ_BYTE
#undef READ_SHORT
#undef READ_CONSTANT
#undef READ_STRING
#undef BINARY_NUMERIC_OP
}

InterpretResult interpret(ObjFunction* entry) {
    push(OBJ_VAL(entry));
    call(entry, 0);
    return run();
}

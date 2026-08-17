#ifndef BCVM_VALUE_H
#define BCVM_VALUE_H

#include "common.h"

/* ---- Tagged value -------------------------------------------------- */

typedef enum {
    VAL_NIL,
    VAL_BOOL,
    VAL_NUMBER,
    VAL_OBJ
} ValueType;

typedef struct Obj Obj;
typedef struct ObjString ObjString;
typedef struct ObjFunction ObjFunction;
typedef struct ObjNative ObjNative;

typedef struct {
    ValueType type;
    union {
        bool boolean;
        double number;
        Obj* obj;
    } as;
} Value;

#define BOOL_VAL(b)    ((Value){VAL_BOOL,   {.boolean = (b)}})
#define NIL_VAL        ((Value){VAL_NIL,    {.number = 0}})
#define NUMBER_VAL(n)  ((Value){VAL_NUMBER, {.number = (n)}})
#define OBJ_VAL(o)     ((Value){VAL_OBJ,    {.obj = (Obj*)(o)}})

#define AS_BOOL(v)     ((v).as.boolean)
#define AS_NUMBER(v)   ((v).as.number)
#define AS_OBJ(v)      ((v).as.obj)

#define IS_BOOL(v)     ((v).type == VAL_BOOL)
#define IS_NIL(v)      ((v).type == VAL_NIL)
#define IS_NUMBER(v)   ((v).type == VAL_NUMBER)
#define IS_OBJ(v)      ((v).type == VAL_OBJ)

/* ---- Heap objects ---------------------------------------------------
 * Every heap object shares this header so the GC can walk them
 * uniformly regardless of concrete type (same idea as a JVM oop header
 * or a CLR object header). */

typedef enum {
    OBJ_STRING,
    OBJ_FUNCTION,
    OBJ_NATIVE
} ObjType;

struct Obj {
    ObjType type;
    bool isMarked;
    struct Obj* next; /* intrusive list of all live allocations */
};

struct ObjString {
    Obj obj;
    int length;
    uint32_t hash;
    char chars[]; /* flexible array member: bytes stored inline */
};

typedef struct Chunk Chunk; /* defined in chunk.h */

struct ObjFunction {
    Obj obj;
    int arity;
    Chunk* chunk;
    ObjString* name;
};

typedef Value (*NativeFn)(int argCount, Value* args);

struct ObjNative {
    Obj obj;
    NativeFn function;
    const char* name;
};

#define OBJ_TYPE(v)     (AS_OBJ(v)->type)
#define IS_STRING(v)    isObjType(v, OBJ_STRING)
#define IS_FUNCTION(v)  isObjType(v, OBJ_FUNCTION)
#define IS_NATIVE(v)    isObjType(v, OBJ_NATIVE)

#define AS_STRING(v)    ((ObjString*)AS_OBJ(v))
#define AS_CSTRING(v)   (((ObjString*)AS_OBJ(v))->chars)
#define AS_FUNCTION(v)  ((ObjFunction*)AS_OBJ(v))
#define AS_NATIVE(v)    (((ObjNative*)AS_OBJ(v))->function)

static inline bool isObjType(Value v, ObjType type) {
    return IS_OBJ(v) && OBJ_TYPE(v) == type;
}

ObjString* copyString(const char* chars, int length);
ObjString* takeString(char* heapChars, int length); /* takes ownership */
ObjFunction* newFunction(void);
ObjNative* newNative(NativeFn fn, const char* name);

bool valuesEqual(Value a, Value b);
void printValue(Value v);
const char* typeName(Value v);

/* ---- Growable Value array (used for constant pools) ----------------- */

typedef struct {
    int count;
    int capacity;
    Value* values;
} ValueArray;

void initValueArray(ValueArray* array);
int writeValueArray(ValueArray* array, Value value);
void freeValueArray(ValueArray* array);

#endif

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "memory.h"
#include "value.h"
#include "vm.h"

void initValueArray(ValueArray* array) {
    array->count = 0;
    array->capacity = 0;
    array->values = NULL;
}

int writeValueArray(ValueArray* array, Value value) {
    if (array->capacity < array->count + 1) {
        int oldCap = array->capacity;
        array->capacity = GROW_CAPACITY(oldCap);
        array->values = GROW_ARRAY(Value, array->values, oldCap, array->capacity);
    }
    array->values[array->count] = value;
    return array->count++;
}

void freeValueArray(ValueArray* array) {
    FREE_ARRAY(Value, array->values, array->capacity);
    initValueArray(array);
}

/* ---- object allocation ---------------------------------------------- */

static Obj* allocateObject(size_t size, ObjType type) {
    Obj* object = (Obj*)reallocate(NULL, 0, size);
    object->type = type;
    object->isMarked = false;
    object->next = vm.objects;
    vm.objects = object;
    return object;
}

static uint32_t hashString(const char* key, int length) {
    /* FNV-1a */
    uint32_t hash = 2166136261u;
    for (int i = 0; i < length; i++) {
        hash ^= (uint8_t)key[i];
        hash *= 16777619u;
    }
    return hash;
}

static ObjString* allocateString(const char* chars, int length, uint32_t hash) {
    ObjString* interned = tableFindString(&vm.strings, chars, length, hash);
    if (interned != NULL) return interned;

    ObjString* str = (ObjString*)allocateObject(sizeof(ObjString) + length + 1, OBJ_STRING);
    str->length = length;
    str->hash = hash;
    memcpy(str->chars, chars, length);
    str->chars[length] = '\0';

    push(OBJ_VAL(str)); /* protect from GC while we touch the table */
    tableSet(&vm.strings, str, NIL_VAL);
    pop();
    return str;
}

ObjString* copyString(const char* chars, int length) {
    return allocateString(chars, length, hashString(chars, length));
}

ObjString* takeString(char* heapChars, int length) {
    /* allocateString always copies into its own flex-array storage (needed so
       that interning/dedup can work uniformly), so the caller's buffer is
       freed unconditionally after we're done with it. */
    uint32_t hash = hashString(heapChars, length);
    ObjString* result = allocateString(heapChars, length, hash);
    free(heapChars);
    return result;
}

ObjFunction* newFunction(void) {
    ObjFunction* fn = (ObjFunction*)allocateObject(sizeof(ObjFunction), OBJ_FUNCTION);
    fn->arity = 0;
    fn->name = NULL;
    fn->chunk = newChunk();
    return fn;
}

ObjNative* newNative(NativeFn function, const char* name) {
    ObjNative* native = (ObjNative*)allocateObject(sizeof(ObjNative), OBJ_NATIVE);
    native->function = function;
    native->name = name;
    return native;
}

/* ---- equality / printing --------------------------------------------- */

bool valuesEqual(Value a, Value b) {
    if (a.type != b.type) return false;
    switch (a.type) {
        case VAL_NIL:    return true;
        case VAL_BOOL:   return AS_BOOL(a) == AS_BOOL(b);
        case VAL_NUMBER: return AS_NUMBER(a) == AS_NUMBER(b);
        case VAL_OBJ:    return AS_OBJ(a) == AS_OBJ(b); /* strings are interned */
    }
    return false;
}

static void printFunction(ObjFunction* fn) {
    if (fn->name == NULL) printf("<script>");
    else printf("<fn %s/%d>", fn->name->chars, fn->arity);
}

void printValue(Value v) {
    switch (v.type) {
        case VAL_NIL:    printf("nil"); break;
        case VAL_BOOL:   printf(AS_BOOL(v) ? "true" : "false"); break;
        case VAL_NUMBER: printf("%g", AS_NUMBER(v)); break;
        case VAL_OBJ:
            switch (OBJ_TYPE(v)) {
                case OBJ_STRING:   printf("%s", AS_CSTRING(v)); break;
                case OBJ_FUNCTION: printFunction(AS_FUNCTION(v)); break;
                case OBJ_NATIVE:   printf("<native %s>", ((ObjNative*)AS_OBJ(v))->name); break;
            }
            break;
    }
}

const char* typeName(Value v) {
    switch (v.type) {
        case VAL_NIL: return "nil";
        case VAL_BOOL: return "bool";
        case VAL_NUMBER: return "number";
        case VAL_OBJ:
            switch (OBJ_TYPE(v)) {
                case OBJ_STRING: return "string";
                case OBJ_FUNCTION: return "function";
                case OBJ_NATIVE: return "native";
            }
    }
    return "?";
}

#include <stdio.h>
#include <stdlib.h>

#include "memory.h"
#include "value.h"
#include "vm.h"

#define GC_HEAP_GROW_FACTOR 2
#define GC_INITIAL_THRESHOLD (1024 * 1024)

void* reallocate(void* pointer, size_t oldSize, size_t newSize) {
    vm.bytesAllocated += newSize - oldSize;

    if (newSize > oldSize) {
#if GC_STRESS
        collectGarbage();
#else
        if (vm.bytesAllocated > vm.nextGC) {
            collectGarbage();
        }
#endif
    }

    if (newSize == 0) {
        free(pointer);
        return NULL;
    }

    void* result = realloc(pointer, newSize);
    if (result == NULL) {
        fprintf(stderr, "bcvm: out of memory\n");
        exit(1);
    }
    return result;
}

static void freeObject(Obj* object) {
    switch (object->type) {
        case OBJ_STRING: {
            ObjString* str = (ObjString*)object;
            reallocate(str, sizeof(ObjString) + str->length + 1, 0);
            break;
        }
        case OBJ_FUNCTION: {
            ObjFunction* fn = (ObjFunction*)object;
            /* fn->chunk is owned by the permanent chunk registry, not freed here */
            FREE(ObjFunction, fn);
            break;
        }
        case OBJ_NATIVE: {
            FREE(ObjNative, object);
            break;
        }
    }
}

void freeObjects(void) {
    Obj* object = vm.objects;
    while (object != NULL) {
        Obj* next = object->next;
        freeObject(object);
        object = next;
    }
    free(vm.grayStack);

    Chunk* chunk = vm.allChunks;
    while (chunk != NULL) {
        Chunk* next = chunk->nextChunk;
        FREE_ARRAY(uint8_t, chunk->code, chunk->capacity);
        FREE_ARRAY(int, chunk->lines, chunk->capacity);
        FREE_ARRAY(Value, chunk->constants.values, chunk->constants.capacity);
        free(chunk); /* chunks embedded in ObjFunction are freed with the fn;
                        top-level chunks allocated with malloc in main are
                        the caller's responsibility -- see registerChunk() */
        chunk = next;
    }
}

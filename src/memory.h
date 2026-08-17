#ifndef BCVM_MEMORY_H
#define BCVM_MEMORY_H

#include "common.h"

#define GROW_CAPACITY(cap) ((cap) < 8 ? 8 : (cap) * 2)

#define GROW_ARRAY(type, ptr, oldCount, newCount) \
    (type*)reallocate(ptr, sizeof(type) * (oldCount), sizeof(type) * (newCount))

#define FREE_ARRAY(type, ptr, oldCount) \
    reallocate(ptr, sizeof(type) * (oldCount), 0)

#define ALLOCATE(type, count) \
    (type*)reallocate(NULL, 0, sizeof(type) * (count))

#define FREE(type, ptr) reallocate(ptr, sizeof(type), 0)

/* Central allocator: every heap byte the engine uses flows through here,
 * which is what lets us track vm.bytesAllocated and trigger the GC. */
void* reallocate(void* pointer, size_t oldSize, size_t newSize);

void freeObjects(void);

#endif

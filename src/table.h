#ifndef BCVM_TABLE_H
#define BCVM_TABLE_H

#include "common.h"
#include "value.h"

typedef struct {
    ObjString* key; /* NULL = empty slot, tombstone marked via value */
    Value value;
} Entry;

typedef struct {
    int count;   /* live entries + tombstones */
    int capacity;
    Entry* entries;
} Table;

void initTable(Table* table);
void freeTable(Table* table);
bool tableGet(Table* table, ObjString* key, Value* value);
bool tableSet(Table* table, ObjString* key, Value value);
bool tableDelete(Table* table, ObjString* key);
void tableAddAll(Table* from, Table* to);
ObjString* tableFindString(Table* table, const char* chars, int length, uint32_t hash);
void markTable(Table* table);
void tableRemoveWhite(Table* table); /* drop unmarked interned strings after sweep */

#endif

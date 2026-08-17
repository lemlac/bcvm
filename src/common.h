#ifndef BCVM_COMMON_H
#define BCVM_COMMON_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Flip to 1 to log every allocation-driven GC cycle to stderr. */
#define GC_LOG 0

/* Flip to 1 to force a collection on every allocation (stress test). */
#define GC_STRESS 0

#endif

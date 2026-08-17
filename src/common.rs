//! Shared constants and configuration flags.

/// Maximum number of call frames.
pub const FRAMES_MAX: usize = 256;

/// Maximum value-stack depth (frames × slots per frame).
pub const STACK_MAX: usize = FRAMES_MAX * 64;

/// Initial GC threshold (bytes).
pub const GC_INITIAL_THRESHOLD: usize = 1024 * 1024;

/// Heap growth factor after a collection.
pub const GC_HEAP_GROW_FACTOR: usize = 2;

/// Load factor threshold for the open-addressing hash table.
pub const TABLE_MAX_LOAD: f64 = 0.75;

/// Grow a capacity value (same strategy as the C original).
#[inline]
pub fn grow_capacity(cap: usize) -> usize {
    if cap < 8 {
        8
    } else {
        cap * 2
    }
}

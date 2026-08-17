//! Open-addressing hash table used for globals and string interning.
//!
//! Keys are always interned `ObjString`s (pointer equality is content equality).
//! Tombstones are marked by a null key + a non-nil value.

use std::rc::Rc;

use crate::common::{grow_capacity, TABLE_MAX_LOAD};
use crate::value::{ObjRef, Value};

#[derive(Debug, Clone)]
struct Entry {
    key: Option<ObjRef>,
    value: Value,
}

impl Entry {
    fn empty() -> Self {
        Entry {
            key: None,
            value: Value::Nil,
        }
    }

    #[allow(dead_code)]
    fn is_tombstone(&self) -> bool {
        self.key.is_none() && !self.value.is_nil()
    }

    fn is_empty_slot(&self) -> bool {
        self.key.is_none() && self.value.is_nil()
    }
}

#[derive(Debug, Default)]
pub struct Table {
    count: usize, // live entries + tombstones
    entries: Vec<Entry>,
}

impl Table {
    pub fn new() -> Self {
        Table {
            count: 0,
            entries: Vec::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.entries.len()
    }

    fn find_entry(entries: &[Entry], key: &ObjRef) -> usize {
        let hash = key.borrow().as_string().hash;
        let capacity = entries.len();
        let mut index = (hash as usize) & (capacity - 1);
        let mut tombstone: Option<usize> = None;

        loop {
            let entry = &entries[index];
            match &entry.key {
                None => {
                    if entry.is_empty_slot() {
                        return tombstone.unwrap_or(index);
                    } else if tombstone.is_none() {
                        tombstone = Some(index);
                    }
                }
                Some(k) if Rc::ptr_eq(k, key) => return index,
                _ => {}
            }
            index = (index + 1) & (capacity - 1);
        }
    }

    fn adjust_capacity(&mut self, capacity: usize) {
        let mut new_entries = vec![Entry::empty(); capacity];
        let mut new_count = 0;

        for src in self.entries.drain(..) {
            if let Some(ref key) = src.key {
                let dest_idx = Self::find_entry(&new_entries, key);
                new_entries[dest_idx] = src;
                new_count += 1;
            }
        }

        self.entries = new_entries;
        self.count = new_count;
    }

    pub fn get(&self, key: &ObjRef) -> Option<Value> {
        if self.count == 0 {
            return None;
        }
        let idx = Self::find_entry(&self.entries, key);
        let entry = &self.entries[idx];
        if entry.key.is_none() {
            None
        } else {
            Some(entry.value.clone())
        }
    }

    /// Returns true if the key was newly inserted.
    pub fn set(&mut self, key: ObjRef, value: Value) -> bool {
        if (self.count + 1) as f64 > self.capacity() as f64 * TABLE_MAX_LOAD {
            let capacity = grow_capacity(self.capacity());
            self.adjust_capacity(capacity);
        }

        let idx = Self::find_entry(&self.entries, &key);
        let is_new_key = self.entries[idx].key.is_none();
        if is_new_key && self.entries[idx].is_empty_slot() {
            self.count += 1;
        }

        self.entries[idx].key = Some(key);
        self.entries[idx].value = value;
        is_new_key
    }

    pub fn delete(&mut self, key: &ObjRef) -> bool {
        if self.count == 0 {
            return false;
        }
        let idx = Self::find_entry(&self.entries, key);
        if self.entries[idx].key.is_none() {
            return false;
        }
        // Leave a tombstone.
        self.entries[idx].key = None;
        self.entries[idx].value = Value::Bool(true);
        true
    }

    /// Look up an interned string by content (used while interning).
    pub fn find_string(&self, chars: &str, hash: u32) -> Option<ObjRef> {
        if self.count == 0 {
            return None;
        }
        let capacity = self.capacity();
        let mut index = (hash as usize) & (capacity - 1);

        loop {
            let entry = &self.entries[index];
            match &entry.key {
                None => {
                    if entry.is_empty_slot() {
                        return None;
                    }
                }
                Some(k) => {
                    let s = k.borrow();
                    let os = s.as_string();
                    if os.length == chars.len()
                        && os.hash == hash
                        && os.chars == chars
                    {
                        return Some(Rc::clone(k));
                    }
                }
            }
            index = (index + 1) & (capacity - 1);
        }
    }

    /// Mark every key and value (GC).
    pub fn mark(&self, mark_obj: &mut dyn FnMut(ObjRef)) {
        for entry in &self.entries {
            if let Some(ref key) = entry.key {
                mark_obj(Rc::clone(key));
            }
            if let Value::Obj(ref o) = entry.value {
                mark_obj(Rc::clone(o));
            }
        }
    }

    /// After the mark phase, drop any interned strings that were not marked.
    pub fn remove_white(&mut self) {
        let mut to_delete = Vec::new();
        for entry in &self.entries {
            if let Some(ref key) = entry.key {
                if !key.borrow().is_marked() {
                    to_delete.push(Rc::clone(key));
                }
            }
        }
        for k in to_delete {
            self.delete(&k);
        }
    }
}

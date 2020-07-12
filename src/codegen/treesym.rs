/*!
A symbol table implemented with hash trees, supporting saving snapshots
*/
use im_rc::{Vector, HashMap};
use std::hash::Hash;

/// A *local* symbol table implemented with hash trees, supporting level intermixing
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LocalTable<K: Eq + Hash + Clone, V: Clone> {
    levels: Vec<HashMap<K, V>>,
    prev: Vec<(K, V)>
}

impl<K: Eq + Hash + Clone, V: Clone> LocalTable<K, V> {
    /// Create a new, empty symbol table
    pub fn new() -> LocalTable<K, V> {
        LocalTable {
            levels: vec![HashMap::new()],
            prev: Vec::new()
        }
    }
    /// Insert a new element into the symbol table at a given level, and every level above it
    pub fn insert(&mut self, key: K, value: V, lower: bool) -> Option<V> {
        if lower {
            self.prev.push((key.clone(), value.clone()))
        }
        self.levels.last_mut().unwrap().insert(key, value)
    }
}

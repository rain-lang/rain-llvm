/*!
A symbol table implemented with hash trees, supporting saving snapshots
*/
use im::HashMap;
use std::hash::Hash;

/// A symbol table implemented with hash trees, supporting mixed levels
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SymbolTable<K: Eq + Hash, V> {
    levels: Vec<HashMap<K, V>>
}

impl<K: Eq + Hash, V> SymbolTable<K, V> {
    /// Create a new, empty symbol table
    pub fn new() -> SymbolTable<K, V> {
        SymbolTable {
            levels: Vec::new()
        }
    }
}

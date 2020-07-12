/*!
A symbol table implemented with hash trees, supporting saving snapshots
*/
use im_rc::HashMap;
use std::hash::Hash;

/// A *local* symbol table implemented with hash trees, supporting mixed levels
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
    // Push a new element onto a symbol table
}

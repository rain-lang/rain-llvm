/*!
Arena of symbol tables for code generation
*/
use super::*;

/// An arena for allocating symbol tables
#[derive(Debug)]
pub struct Arena<'ctx> {
    /// The underlying data of this arena
    data: Vec<Either<SymbolTable<ValId, Val<'ctx>>, usize>>,
    /// The head of the free list for this arena
    free_head: usize,
}

impl<'ctx> Arena<'ctx> {
    /// Create a new, empty arena
    pub fn new() -> Arena<'ctx> {
        Arena {
            data: Vec::new(),
            free_head: usize::MAX,
        }
    }

    /// Get a mutable reference to the symbol table stored at a given index, if any
    pub fn get_mut(&mut self, ix: usize) -> Option<&mut SymbolTable<ValId, Val<'ctx>>> {
        if ix >= self.data.len() {
            None
        } else {
            match &mut self.data[ix] {
                Either::Left(t) => Some(t),
                Either::Right(_) => None,
            }
        }
    }

    /// Get an immutable reference to the symbol table stored at a given index, if any
    pub fn get(&mut self, ix: usize) -> Option<&SymbolTable<ValId, Val<'ctx>>> {
        if ix >= self.data.len() {
            None
        } else {
            match &self.data[ix] {
                Either::Left(l) => Some(l),
                Either::Right(_) => None,
            }
        }
    }

    /// Push a new symbol table into the arena, forcing it to be placed at the end. Returns the new index
    ///
    /// Note this is never *wrong* to call, it just doesn't re-use memory in the free list even if this is possible
    pub fn push_end(&mut self, table: SymbolTable<ValId, Val<'ctx>>) -> usize {
        let ix = self.data.len();
        self.data.push(Either::Left(table));
        ix
    }

    /// Push a new symbol table into the arena, returning it's index
    pub fn push(&mut self, table: SymbolTable<ValId, Val<'ctx>>) -> usize {
        // Check if the free list is empty
        if self.free_head == usize::MAX {
            return self.push_end(table);
        }
        // Get the next element of the free list.
        let next_free = match &self.data[self.free_head] {
            Either::Left(_) => panic!("Free head {} points to data", self.free_head),
            Either::Right(r) => *r,
        };
        // Overwrite the head of the free list
        self.data[self.free_head] = Either::Left(table);
        // Advance the free list
        let ix = self.free_head;
        self.free_head = next_free;
        // Return the new index
        ix
    }

    /// Free a symbol table from the arena, returning it if it existed and updating the free head appropriately
    pub fn free(&mut self, ix: usize) -> Option<SymbolTable<ValId, Val<'ctx>>> {
        // If the index is out of bounds or already freed, return `None`
        if ix > self.data.len() || self.data[ix].is_right() {
            return None;
        }
        // Otherwise, create a new linked list node with the free list head as previous node, swapping out the old data
        let mut new_node = Either::Right(self.free_head);
        std::mem::swap(&mut new_node, &mut self.data[ix]);
        // Then update the free list to have the new node as a head
        self.free_head = ix;
        // Finally, return the old data, extracted from it's Either
        new_node.left()
    }

    /// Get the capacity of this arena
    pub fn capacity(&self) -> usize {
        self.data.len()
    }
}

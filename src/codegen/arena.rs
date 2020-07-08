/*!
Arena of symbol table for code generation
*/
use super::*;

#[derive(Debug)]
pub struct Arena<'ctx>(Vec<Either<SymbolTable<ValId, Val<'ctx>>, usize>>);

impl<'ctx> Arena<'ctx> {
    pub fn new() -> Arena<'ctx> {
        Arena{
            0: Vec::new()
        }
    }

    pub fn get_mut_table(&mut self, ix: usize) -> Option<&mut SymbolTable<ValId, Val<'ctx>>> {
        if ix >= self.0.len() {
            None
        } else {
            match &mut self.0[ix] {
                Either::Left(t) => Some(t),
                Either::Right(_) => None
            }
        }
    }

    pub fn set(&mut self, ix: usize, payload: Either<SymbolTable<ValId, Val<'ctx>>, usize>) {
        self.0[ix] = payload;
    }

    pub fn get(&mut self, ix: usize) -> Option<&Either<SymbolTable<ValId, Val<'ctx>>, usize>> {
        if ix >= self.0.len() {
            None
        } else {
            Some(&self.0[ix])
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn push_table(&mut self, table: SymbolTable<ValId, Val<'ctx>>) {
        self.0.push(Either::Left(table));
    }

}
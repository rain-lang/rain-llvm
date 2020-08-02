/*!
LLVM representations for rain types and values
*/

use inkwell::types::{BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue};
use std::convert::TryFrom;
use std::ops::Deref;
use std::rc::Rc;

/**
A representation of product
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRepr<'ctx> {
    /// A mapping since we need to skip Repr::Unit
    ///
    /// `mapping[i]` holds the position of ith element in the struct
    pub mapping: IxMap,
    /// The actual representation
    pub repr: StructType<'ctx>,
}

/**
A LLVM function implementing a main function
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionRepr<'ctx> {
    /// A mapping since we need to skip `Repr::Unit`
    pub mapping: IxMap,
    /// The function type representation
    pub repr: FunctionType<'ctx>,
}

/**
An LLVM representation for a `rain` type
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repr<'ctx> {
    /// As a basic LLVM type
    Type(BasicTypeEnum<'ctx>),
    /// As a function
    Function(Rc<FunctionRepr<'ctx>>),
    /// As a compound
    Product(Rc<ProductRepr<'ctx>>),
    /// As a mere proposition
    Prop,
    /// As the empty type
    Empty,
}

/**
An LLVM representation of a value for a `rain` type
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val<'ctx> {
    /// As a basic LLVM value
    Value(BasicValueEnum<'ctx>),
    /// As a function
    Function(FunctionValue<'ctx>),
    /// A value of the unit type, indicating a no-op`
    Unit,
    /// A contradiction, indicating undefined behaviour
    Contr,
}

impl<'ctx> From<BasicValueEnum<'ctx>> for Val<'ctx> {
    #[inline]
    fn from(b: BasicValueEnum<'ctx>) -> Val<'ctx> {
        Val::Value(b)
    }
}

impl<'ctx> TryFrom<Val<'ctx>> for BasicValueEnum<'ctx> {
    type Error = Val<'ctx>;
    #[inline]
    fn try_from(v: Val<'ctx>) -> Result<BasicValueEnum<'ctx>, Val<'ctx>> {
        match v {
            Val::Value(v) => Ok(v),
            v => Err(v),
        }
    }
}

impl<'ctx> From<IntValue<'ctx>> for Val<'ctx> {
    #[inline]
    fn from(i: IntValue<'ctx>) -> Val<'ctx> {
        Val::Value(i.into())
    }
}

impl<'ctx> From<FunctionValue<'ctx>> for Val<'ctx> {
    #[inline]
    fn from(f: FunctionValue<'ctx>) -> Val<'ctx> {
        Val::Function(f)
    }
}

impl<'ctx> TryFrom<Val<'ctx>> for IntValue<'ctx> {
    type Error = Val<'ctx>;
    #[inline]
    fn try_from(v: Val<'ctx>) -> Result<IntValue<'ctx>, Val<'ctx>> {
        match v {
            Val::Value(BasicValueEnum::IntValue(v)) => Ok(v),
            v => Err(v),
        }
    }
}

impl<'ctx> TryFrom<Val<'ctx>> for FunctionValue<'ctx> {
    type Error = Val<'ctx>;
    #[inline]
    fn try_from(v: Val<'ctx>) -> Result<FunctionValue<'ctx>, Val<'ctx>> {
        match v {
            Val::Function(f) => Ok(f),
            v => Err(v),
        }
    }
}

/// An enumeration for indices into an LLVM representation
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ReprIx {
    /// A value index
    Val(u32),
    /// A propositional index
    Prop,
}

impl ReprIx {
    /// The index corresponding to a propositional type
    pub const PROP_IX: i32 = -1;
}

impl From<i32> for ReprIx {
    fn from(ix: i32) -> ReprIx {
        match ix {
            ReprIx::PROP_IX => ReprIx::Prop,
            ix => ReprIx::Val(ix as u32),
        }
    }
}

/// A map of indices into a parameter array to representation indices
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IxMap(Vec<i32>);

impl Default for IxMap {
    fn default() -> IxMap {
        IxMap::new()
    }
}

impl IxMap {
    /// Create a new set of input indices
    pub fn new() -> IxMap {
        Self::with_capacity(0)
    }
    /// Create a new set of input indices with the given capacity
    pub fn with_capacity(n: u32) -> IxMap {
        IxMap(Vec::with_capacity(n as usize))
    }
    /// Push a new value index
    pub fn push_ix(&mut self, ix: u32) {
        self.0.push(ix as i32)
    }
    /// Push a new propositional index
    pub fn push_prop(&mut self) {
        self.0.push(ReprIx::PROP_IX)
    }
    /// Push a new input index
    pub fn push(&mut self, ix: ReprIx) {
        match ix {
            ReprIx::Val(val) => self.push_ix(val),
            ReprIx::Prop => self.push_prop(),
        }
    }
    /// Iterate over the elements of this set of input indices
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = ReprIx> + ExactSizeIterator + DoubleEndedIterator + '_ {
        self.0.iter().map(|ix| (*ix).into())
    }
    /// Get the index associated with a value
    pub fn get(&self, ix: usize) -> Option<u32> {
        self.0
            .get(ix)
            .map(|ix| {
                if let ReprIx::Val(ix) = ReprIx::from(*ix) {
                    Some(ix)
                } else {
                    None
                }
            })
            .flatten()
    }
}

impl Deref for IxMap {
    type Target = [i32];
    #[inline]
    fn deref(&self) -> &[i32] {
        &self.0
    }
}

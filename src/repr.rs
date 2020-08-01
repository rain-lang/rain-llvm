/*!
LLVM representations for rain types and values
*/

use inkwell::types::{BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue};
use std::convert::TryFrom;
use std::rc::Rc;

/**
A representation of product
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRepr<'ctx> {
    /// A mapping since we need to skip Repr::Unit
    ///
    /// `mapping[i]` holds the position of ith element in the struct
    pub mapping: Vec<Option<u32>>,
    /// The actual representation
    pub repr: StructType<'ctx>,
}

/**
An LLVM representation for a `rain` type
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repr<'ctx> {
    /// As a basic LLVM type
    Type(BasicTypeEnum<'ctx>),
    /// As a function
    Function(FunctionType<'ctx>),
    /// As a compound
    Product(Rc<ProductRepr<'ctx>>),
    /// As a mere proposition
    Prop,
    /// As the empty type
    Empty,
    /// An irrepresentable type
    Irrep,
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
    /// An irrepresentable value but valid value, propagating to a runtime error if not in an unreachable branch
    Irrep,
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

/*!
LLVM representations for rain types and values
*/

use inkwell::types::{BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{BasicValueEnum, FunctionValue};

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
    Product(ProductRepr<'ctx>),
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

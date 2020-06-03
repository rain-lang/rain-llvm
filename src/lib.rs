/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use fxhash::FxHashMap as HashMap;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{AnyValueEnum, BasicValueEnum, FunctionValue};
use rain_lang::value::{lifetime::Live, TypeId, ValId, ValueEnum};

/**
A local `rain` value
*/
#[derive(Debug, Clone)]
pub enum Local<'ctx> {
    /// A normal value: the result of an instruction
    Value(AnyValueEnum<'ctx>),
    /// A unit value
    Unit,
    /// A contradiction: undefined behaviour
    Contradiction,
}

/**
A constant `rain` value or function
*/
#[derive(Debug, Clone)]
pub enum Const<'ctx> {
    /// A normal constant value
    Value(BasicValueEnum<'ctx>),
    /// A compiled function
    Function(FunctionValue<'ctx>),
}

/**
A representation for a `rain` type
*/
#[derive(Debug, Clone)]
pub enum Repr<'ctx> {
    /// As a basic LLVM type
    Type(BasicTypeEnum<'ctx>),
    /// As the unit type
    Unit,
    /// As the empty type
    Empty,
    /// An irrepresentable type
    Irrepresentable,
}

/**
A `rain` code generation context for a single function
*/
#[derive(Debug, Clone)]
pub struct FnCtx<'ctx> {
    /// Values defined for this function
    locals: HashMap<ValId, Local<'ctx>>,
    /// The function for which this context is defined
    func: FunctionValue<'ctx>,
}

/**
A `rain` code generation context for a given module
*/
#[derive(Debug)]
pub struct Codegen<'ctx> {
    /// Global compiled values
    vals: HashMap<ValId, AnyValueEnum<'ctx>>,
    /// Type representations
    reprs: HashMap<TypeId, Repr<'ctx>>,
    /// The module being generated
    module: Module<'ctx>,
    /// The builder being used
    builder: Builder<'ctx>,
    /// The enclosing context of this codegen context
    context: &'ctx Context,
}

/// A `rain` code generation error
#[derive(Debug, Clone)]
pub enum Error {
    /// Attempted to create a non-constant value as a constant
    NotConst,
    /// Attempted to create a non-constant value of an irrepresentable type
    Irrepresentable,
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context
    pub fn new(context: &'ctx Context, module_name: &str) -> Codegen<'ctx> {
        Codegen {
            vals: HashMap::default(),
            reprs: HashMap::default(),
            module: context.create_module(module_name),
            builder: context.create_builder(),
            context,
        }
    }
    /// Get the representation for a given type, if any
    pub fn get_repr(&mut self, t: &TypeId) -> Result<Repr<'ctx>, Error> {
        //TODO: caching, `Entry`, etc...
        match t.as_enum() {
            ValueEnum::BoolTy(_) => Ok(Repr::Type(self.context.bool_type().into())),
            _ => unimplemented!(),
        }
    }
    /// Get a compiled constant `rain` value or function
    pub fn get_const(&mut self, v: &ValId) -> Result<Const<'ctx>, Error> {
        if v.lifetime().region().is_some() {
            return Err(Error::NotConst);
        }
        unimplemented!()
    }
}

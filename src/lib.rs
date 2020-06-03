/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use fxhash::FxHashMap as HashMap;
use inkwell::basic_block::BasicBlock;
use inkwell::types::AnyTypeEnum;
use inkwell::values::{BasicValueEnum, FunctionValue, GlobalValue};
use inkwell::module::Module;
use inkwell::context::Context;
use inkwell::builder::Builder;
use rain_lang::value::{TypeId, ValId};

/**
A `rain` code generation context for a single function
*/
#[derive(Debug, Clone)]
pub struct FnCtx<'ctx> {
    /// The basic blocks of sub-functions of this function
    blocks: HashMap<ValId, BasicBlock<'ctx>>,
    /// Values defined for this function
    vals: HashMap<ValId, BasicValueEnum<'ctx>>,
    /// The function for which this context is defined
    func: FunctionValue<'ctx>,
}

/**
A `rain` code generation context for a given module
*/
#[derive(Debug)]
pub struct Codegen<'ctx> {
    /// Compiled functions
    fns: HashMap<ValId, FnCtx<'ctx>>,
    /// Global values
    globals: HashMap<ValId, GlobalValue<'ctx>>,
    /// Types
    tys: HashMap<TypeId, AnyTypeEnum<'ctx>>,
    /// The module being generated
    module: Module<'ctx>,
    /// The builder being used
    builder: Builder<'ctx>,
    /// The enclosing context of this codegen context
    context: &'ctx Context
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context
    pub fn new(context: &'ctx Context, module_name: &str) -> Codegen<'ctx> {
        Codegen {
            fns: HashMap::default(),
            globals: HashMap::default(),
            tys: HashMap::default(),
            module: context.create_module(module_name),
            builder: context.create_builder(),
            context
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

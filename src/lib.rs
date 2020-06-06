/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use fxhash::FxHashMap as HashMap;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Module, Linkage};
use inkwell::types::{BasicTypeEnum, FunctionType};
use inkwell::values::{AnyValueEnum, BasicValueEnum, FunctionValue};
use rain_lang::value::{
    function::{lambda::Lambda, pi::Pi},
    lifetime::Live,
    TypeId, ValId, ValueEnum,
};
use std::ops::Deref;

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
    /// A unit value
    Unit,
    /// A contradiction: undefined behaviour. This means the program made a wrong assumption!
    Contradiction,
}

/**
A representation for a `rain` type
*/
#[derive(Debug, Clone)]
pub enum Repr<'ctx> {
    /// As a basic LLVM type
    Type(BasicTypeEnum<'ctx>),
    /// As a function
    Function(FunctionType<'ctx>),
    /// As the unit type
    Unit,
    /// As the empty type
    Empty,
    /// An irrepresentable type
    Irrepresentable,
}

/**
A local `rain` code generation context
*/
#[derive(Debug, Clone)]
pub struct LocalCtx<'ctx> {
    /// Values defined for this function
    locals: HashMap<ValId, Local<'ctx>>,
    /// The function for which this context is defined
    func: FunctionValue<'ctx>,
}

impl<'ctx> LocalCtx<'ctx> {
    /// Crate a new code generation context with a given function as base
    pub fn new(func: FunctionValue<'ctx>) -> LocalCtx<'ctx> {
        LocalCtx {
            locals: HashMap::default(),
            func
        }
    }
}

/**
A global `rain` code generation context for a given codegen module
*/
#[derive(Debug, Clone)]
pub struct GlobalCtx<'ctx> {
    /// Global compiled values
    vals: HashMap<ValId, AnyValueEnum<'ctx>>,
    /// Type representations
    reprs: HashMap<TypeId, Repr<'ctx>>,
}

impl<'ctx> GlobalCtx<'ctx> {
    /// Create a new, empty global context
    pub fn new() -> GlobalCtx<'ctx> {
        GlobalCtx {
            vals: HashMap::default(),
            reprs: HashMap::default(),
        }
    }
}

/**
A `rain` code generation context for a given module
*/
#[derive(Debug)]
pub struct Codegen<'ctx> {
    /// The global code generation context for the module being generated
    global: GlobalCtx<'ctx>,
    /// Function name counter
    counter: usize,
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
    /// Invalid function representation
    InvalidFuncRepr,
    /// Not implemented
    NotImplemented(&'static str),
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context
    pub fn new(context: &'ctx Context, module_name: &str) -> Codegen<'ctx> {
        Codegen {
            global: GlobalCtx::new(),
            counter: 0,
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
    /// Create a function prototype for a constant pi type
    pub fn const_pi_prototype(&mut self, pi: &Pi) -> Result<Repr<'ctx>, Error> {
        if pi.lifetime().depth() != 0 {
            return Err(Error::NotConst);
        }
        let result = pi.result();
        if result.lifetime().depth() != 0 {
            return Err(Error::NotImplemented(
                "Non-constant return types for pi functions",
            ));
        }
        let result_repr = match self.get_repr(result)? {
            Repr::Type(t) => t,
            Repr::Function(_f) => unimplemented!(),
            Repr::Empty | Repr::Unit => return Ok(Repr::Unit),
            Repr::Irrepresentable => return Err(Error::Irrepresentable),
        };
        let input_reprs: Result<Vec<_>, _> = pi
            .def_region()
            .iter()
            .filter_map(|ty| match self.get_repr(ty) {
                Ok(Repr::Type(t)) => Some(Ok(t)),
                Err(e) => Some(Err(e)),
                _ => None,
            })
            .collect();
        let input_reprs = input_reprs?;

        // Construct our prototype
        let result_fn = match result_repr {
            BasicTypeEnum::ArrayType(a) => a.fn_type(&input_reprs, false),
            BasicTypeEnum::FloatType(f) => f.fn_type(&input_reprs, false),
            BasicTypeEnum::IntType(i) => i.fn_type(&input_reprs, false),
            BasicTypeEnum::PointerType(p) => p.fn_type(&input_reprs, false),
            BasicTypeEnum::StructType(s) => s.fn_type(&input_reprs, false),
            BasicTypeEnum::VectorType(v) => v.fn_type(&input_reprs, false),
        };
        Ok(Repr::Function(result_fn))
    }

    /// Compile a return value into a function context
    pub fn compile_retv(&mut self, ctx: &mut LocalCtx<'ctx>, v: &ValId) -> Result<(), Error> {
        unimplemented!()
    }

    /// Compile a constant `rain` lambda function
    pub fn compile_const_lambda(&mut self, l: &Lambda) -> Result<Const<'ctx>, Error> {
        let ty = l.get_ty();
        let prototype = self.const_pi_prototype(ty.deref())?;

        match prototype {
            Repr::Function(fn_ty) => {
                // Construct an empty function with the prototype
                let fn_val = self.module.add_function(
                    &format!("_private_lambda_{}", self.counter),
                    fn_ty,
                    Some(Linkage::Private)
                );
                let mut ctx = LocalCtx::new(fn_val);
                self.compile_retv(&mut ctx, l.result())?;
                Ok(Const::Function(fn_val))
            },
            Repr::Unit => Ok(Const::Unit),
            _ => Err(Error::InvalidFuncRepr)
        }

    }
    /// Get a compiled constant `rain` value or function
    pub fn compile_const(&mut self, v: &ValueEnum) -> Result<Const<'ctx>, Error> {
        match v {
            ValueEnum::BoolTy(_) => Ok(Const::Unit),
            ValueEnum::Bool(b) => Ok(Const::Value(
                self.context.bool_type().const_int(*b as u64, false).into(),
            )),
            ValueEnum::Finite(_) => Ok(Const::Unit),
            ValueEnum::Index(_i) => unimplemented!(),
            ValueEnum::Lambda(l) => self.compile_const_lambda(l),
            ValueEnum::Pi(_p) => unimplemented!(),
            ValueEnum::Gamma(_g) => unimplemented!(),
            ValueEnum::Phi(_p) => unimplemented!(),
            ValueEnum::Parameter(_) => Err(Error::NotConst),
            ValueEnum::Product(_p) => unimplemented!(),
            ValueEnum::Tuple(_t) => unimplemented!(),
            ValueEnum::Sexpr(_s) => unimplemented!(),
            ValueEnum::Universe(_) => Ok(Const::Unit),
        }
    }
}

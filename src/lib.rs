/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use fxhash::FxHashMap as HashMap;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicTypeEnum, FunctionType};
use inkwell::values::{AnyValueEnum, BasicValue, BasicValueEnum, FunctionValue, InstructionValue};
use rain_lang::value::{
    function::{lambda::Lambda, pi::Pi},
    lifetime::{Live, Region},
    TypeId, ValId, ValueEnum,
};
use std::ops::Deref;

/**
A local `rain` value
*/
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum Local<'ctx> {
    /// A normal value: the result of an instruction
    Value(AnyValueEnum<'ctx>),
    /// A unit value
    Unit,
    /// A contradiction: undefined behaviour
    Contradiction,
}

impl<'ctx> From<Const<'ctx>> for Local<'ctx> {
    #[inline]
    fn from(c: Const<'ctx>) -> Local<'ctx> {
        match c {
            Const::Value(v) => Local::Value(v.into()),
            Const::Function(f) => Local::Value(f.into()),
            Const::Unit => Local::Unit,
            Const::Contradiction => Local::Contradiction,
        }
    }
}

/**
A constant `rain` value or function
*/
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
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
#[derive(Debug)]
pub struct LocalCtx<'ctx> {
    /// Values defined for this function
    locals: HashMap<ValId, Local<'ctx>>,
    /// The region associated with this local context
    region: Region,
    /// The function for which this context is defined
    func: FunctionValue<'ctx>,
}

impl<'ctx> LocalCtx<'ctx> {
    /// Crate a new code generation context with a given function as base
    pub fn new(
        _codegen: &Codegen<'ctx>,
        region: Region,
        func: FunctionValue<'ctx>,
    ) -> LocalCtx<'ctx> {
        LocalCtx {
            locals: HashMap::default(),
            region,
            func,
        }
    }
}

/**
A `rain` code generation context for a given module
*/
#[derive(Debug)]
pub struct Codegen<'ctx> {
    /// Global compiled values
    consts: HashMap<ValId, Const<'ctx>>,
    /// Type representations
    reprs: HashMap<TypeId, Repr<'ctx>>,
    /// Function name counter
    counter: usize,
    /// The module being generated
    module: Module<'ctx>,
    /// The IR builder for this local context
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
    /// An internal error
    InternalError(&'static str),
    /// Not implemented
    NotImplemented(&'static str),
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context
    pub fn new(context: &'ctx Context, module_name: &str) -> Codegen<'ctx> {
        Codegen {
            consts: HashMap::default(),
            reprs: HashMap::default(),
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
    pub fn compile_retv(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        v: &ValId,
    ) -> Result<InstructionValue<'ctx>, Error> {
        let retv = self.compile(ctx, v)?;
        let undef_retv: Option<BasicValueEnum> = match retv {
            Local::Unit | Local::Contradiction => {
                ctx.func.get_type().get_return_type().map(|ty| match ty {
                    BasicTypeEnum::ArrayType(a) => a.get_undef().into(),
                    BasicTypeEnum::FloatType(f) => f.get_undef().into(),
                    BasicTypeEnum::IntType(i) => i.get_undef().into(),
                    BasicTypeEnum::PointerType(p) => p.get_undef().into(),
                    BasicTypeEnum::StructType(s) => s.get_undef().into(),
                    BasicTypeEnum::VectorType(v) => v.get_undef().into(),
                })
            }
            _ => None,
        };
        let retv_borrow = match &retv {
            Local::Value(v) => match v {
                AnyValueEnum::ArrayValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::IntValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::FloatValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::PhiValue(_) => unimplemented!(),
                AnyValueEnum::FunctionValue(_) => unimplemented!(),
                AnyValueEnum::PointerValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::StructValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::VectorValue(v) => Some(v as &dyn BasicValue),
                AnyValueEnum::InstructionValue(_) => unimplemented!(),
            },
            Local::Unit | Local::Contradiction => undef_retv.as_ref().map(|v| v as &dyn BasicValue),
        };
        let ret = self.builder.build_return(retv_borrow);
        Ok(ret)
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
                    Some(Linkage::Private),
                );
                let mut ctx = LocalCtx::new(self, l.def_region().clone(), fn_val);
                self.compile_retv(&mut ctx, l.result())?;
                Ok(Const::Function(fn_val))
            }
            Repr::Unit => Ok(Const::Unit),
            _ => Err(Error::InvalidFuncRepr),
        }
    }

    /// Get a compiled constant `rain` value or function
    pub fn compile_const_enum(&mut self, v: &ValueEnum) -> Result<Const<'ctx>, Error> {
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

    /// Get a compiled constant `rain` value
    pub fn compile_const(&mut self, v: &ValId) -> Result<Const<'ctx>, Error> {
        if let Some(c) = self.consts.get(v) {
            Ok(*c)
        } else {
            let c = self.compile_const_enum(v.as_enum())?;
            self.consts.insert(v.clone(), c);
            Ok(c)
        }
    }

    /// Compile a `ValueEnum` in a local context
    pub fn compile_enum(
        &mut self,
        _ctx: &mut LocalCtx<'ctx>,
        v: &ValueEnum,
    ) -> Result<Local<'ctx>, Error> {
        match v {
            v @ ValueEnum::BoolTy(_)
            | v @ ValueEnum::Bool(_)
            | v @ ValueEnum::Finite(_)
            | v @ ValueEnum::Index(_)
            | v @ ValueEnum::Universe(_) => self.compile_const_enum(v).map(Local::from),
            ValueEnum::Lambda(_l) => unimplemented!(),
            ValueEnum::Pi(_p) => unimplemented!(),
            ValueEnum::Gamma(_g) => unimplemented!(),
            ValueEnum::Phi(_p) => unimplemented!(),
            ValueEnum::Parameter(_) => unimplemented!(),
            ValueEnum::Product(_p) => unimplemented!(),
            ValueEnum::Tuple(_t) => unimplemented!(),
            ValueEnum::Sexpr(_s) => unimplemented!(),
        }
    }

    /// Compile a `ValId` in a local context
    pub fn compile(&mut self, ctx: &mut LocalCtx<'ctx>, v: &ValId) -> Result<Local<'ctx>, Error> {
        // NOTE: compiling a constant never leaves the builder, and we do not implemented nested regions currently.
        // Hence, we work with the (bad) assumption that we never have to worry about the builder jumping around because
        // any serialization respecting dependency order is a valid serialization. Note we *also* ignore lifetime order,
        // but we'll fix that when we implement DFS
        if ctx.region.depth() > 1 {
            return Err(Error::NotImplemented("Nested region ValId compilation"));
        }
        // Constant regions are constants
        if v.lifetime().depth() == 0 {
            return self.compile_const(v).map(Local::from);
        } else if let Some(l) = ctx.locals.get(v) {
            // Check the local cache
            return Ok(*l);
        }
        let result = self.compile_enum(ctx, v.as_enum())?;
        ctx.locals.insert(v.clone(), result);
        Ok(result)
    }
}

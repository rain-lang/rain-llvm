/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use fxhash::FxHashMap as HashMap;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{AnyValueEnum, BasicValue, BasicValueEnum, FunctionValue, InstructionValue};
use rain_lang::value::{
    function::{lambda::Lambda, pi::Pi},
    lifetime::{Live, Parameter, Region},
    tuple::Tuple,
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
    /// An irrepresentable value
    Irrep,
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
    /// An irrepresentable value
    Irrep,
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
    /// As a mere proposition
    Prop,
    /// As the empty type
    Empty,
    /// An irrepresentable type
    Irrep,
}

/**
A function prototype
*/
#[derive(Debug)]
pub enum Prototype<'ctx> {
    /// A local context to build the function in
    Ctx(LocalCtx<'ctx>),
    /// A marker indicating this function is a mere proposition
    Prop,
    /// A marker indicating this function is irrepresentable
    Irrep,
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
    pub fn const_pi_prototype(&mut self, pi: &Pi) -> Result<Prototype<'ctx>, Error> {
        if pi.lifetime().depth() != 0 {
            return Err(Error::NotConst);
        }
        let region = pi.def_region();
        let result = pi.result();
        if result.lifetime().depth() != 0 {
            return Err(Error::NotImplemented(
                "Non-constant return types for pi functions",
            ));
        }
        let result_repr = match self.get_repr(result)? {
            Repr::Type(t) => t,
            Repr::Function(_f) => unimplemented!(),
            Repr::Empty | Repr::Prop => return Ok(Prototype::Prop),
            Repr::Irrep => return Ok(Prototype::Irrep),
        };
        let mut input_reprs: Vec<BasicTypeEnum> = Vec::with_capacity(region.len());
        let mut input_ixes: Vec<isize> = Vec::with_capacity(region.len());
        const PROP_IX: isize = -1;
        const EMPTY_IX: isize = -2;
        const IRREP_IX: isize = -3;

        for (i, input_ty) in region.iter().enumerate() {
            match self.get_repr(input_ty)? {
                Repr::Type(t) => {
                    input_ixes.push(input_reprs.len() as isize);
                    input_reprs.push(t);
                }
                Repr::Prop => {
                    input_ixes.push(PROP_IX);
                }
                Repr::Empty => {
                    input_ixes.push(EMPTY_IX);
                }
                Repr::Irrep => {
                    input_ixes.push(IRREP_IX);
                }
            }
        }

        // Construct a function type
        let result_ty = result_repr.fn_type(&input_reprs, false);

        // Construct an empty function of a given type
        let result_fn = self.module.add_function(
            format!("__lambda_{}", self.counter),
            result_ty,
            Some(Linkage::Private),
        );
        self.counter += 1;

        // Construct a context, binding local values to types
        let mut ctx = LocalCtx::new(self, region.clone(), result_fn);
        for (i, ix) in input_ixes.iter().copied().enumerate() {
            let param = ValId::from(region.param(i).expect("Iterated index is in bounds"));
            match ix {
                PROP_IX => ctx.locals.insert(param, Local::Unit),
                EMPTY_IX => ctx.locals.insert(param, Local::Contradiction),
                IRREP_IX => ctx.locals.insert(param, Local::Irrep),
                ix if ix >= 0 && ix < input_reprs.len() as isize => ctx.locals.insert(
                    param,
                    Local::Value(
                        result_fn
                            .get_nth_param(ix as u32)
                            .expect("Index in vector is in bounds")
                            .into(),
                    ),
                ),
                n => panic!("Invalid representation index {}!", n),
            }
        }

        Ok(Prototype::Ctx(ctx))
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
            Prototype::Ctx(ctx) => {
                self.compile_retv(&mut ctx, l.result())?;
                Ok(Const::Function(ctx.func))
            }
            Prototype::Prop => Ok(Const::Unit),
            Prototype::Irrep => Ok(Const::Irrep),
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

    /// Compile a parameter in a local context
    pub fn compile_parameter(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        p: &Parameter,
    ) -> Result<Local<'ctx>, Error> {
        ctx.locals
            .get(&ValId::from(p.clone()))
            .cloned()
            .ok_or(Error::InternalError(
                "Context should have parameters pre-registered!s",
            ))
    }

    /// Compile a tuple in a local context
    pub fn compile_tuple(&mut self, ctx: &mut LocalCtx<'ctx>, p: &Tuple) -> Result<Local<'ctx>, Error> {
        unimplemented!()
    }

    /// Compile a `ValueEnum` in a local context
    pub fn compile_enum(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
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
            ValueEnum::Parameter(p) => self.compile_parameter(ctx, p),
            ValueEnum::Product(_) => unimplemented!(),
            ValueEnum::Tuple(t) => self.compile_tuple(ctx, t),
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

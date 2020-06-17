/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

use either::Either;
use fxhash::FxHashMap as HashMap;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{
    AnyValueEnum, BasicValue, BasicValueEnum, FunctionValue, InstructionValue, IntValue,
};
use rain_lang::function::{lambda::Lambda, pi::Pi};
use rain_lang::region::Regional;
use rain_lang::primitive::logical::{self, Logical, LOGICAL_OP_TYS};
use rain_lang::primitive::finite::{Finite, Index};
use rain_lang::region::{Parameter, Region};
use rain_lang::value::{expr::Sexpr, tuple::Tuple, TypeId, ValId, ValueEnum};
use std::convert::{TryFrom, TryInto};
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
            Const::Irrep => Local::Irrep,
        }
    }
}

impl<'ctx> From<AnyValueEnum<'ctx>> for Local<'ctx> {
    #[inline]
    fn from(a: AnyValueEnum<'ctx>) -> Local<'ctx> {
        Local::Value(a)
    }
}

impl<'ctx> From<BasicValueEnum<'ctx>> for Local<'ctx> {
    #[inline]
    fn from(b: BasicValueEnum<'ctx>) -> Local<'ctx> {
        Local::Value(b.into())
    }
}

impl<'ctx> From<IntValue<'ctx>> for Local<'ctx> {
    #[inline]
    fn from(i: IntValue<'ctx>) -> Local<'ctx> {
        Local::Value(i.into())
    }
}

impl<'ctx> TryFrom<Local<'ctx>> for IntValue<'ctx> {
    type Error = Local<'ctx>;
    fn try_from(l: Local<'ctx>) -> Result<IntValue<'ctx>, Local<'ctx>> {
        match l {
            Local::Value(a) => a.try_into().map_err(|_| l),
            _ => Err(l),
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

impl<'ctx> From<FunctionValue<'ctx>> for Const<'ctx> {
    fn from(f: FunctionValue<'ctx>) -> Const<'ctx> {
        Const::Function(f)
    }
}

impl<'ctx> From<BasicValueEnum<'ctx>> for Const<'ctx> {
    #[inline]
    fn from(b: BasicValueEnum<'ctx>) -> Const<'ctx> {
        Const::Value(b.into())
    }
}

impl<'ctx> From<IntValue<'ctx>> for Const<'ctx> {
    #[inline]
    fn from(i: IntValue<'ctx>) -> Const<'ctx> {
        Const::Value(i.into())
    }
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
    /// The basic block at the head of this local context
    head: Option<BasicBlock<'ctx>>,
    /// The function for which this context is defined
    func: FunctionValue<'ctx>,
}

impl<'ctx> LocalCtx<'ctx> {
    /// Crate a new code generation context with a given function and basic block as base
    pub fn new(
        _codegen: &mut Codegen<'ctx>,
        region: Region,
        func: FunctionValue<'ctx>,
        head: Option<BasicBlock<'ctx>>,
    ) -> LocalCtx<'ctx> {
        LocalCtx {
            locals: HashMap::default(),
            region,
            func,
            head,
        }
    }
    /// Get the head of this function. If this function has no head, generate an entry block
    pub fn get_head(&mut self, codegen: &mut Codegen<'ctx>) -> BasicBlock<'ctx> {
        if let Some(head) = self.head {
            return head;
        }
        let head = codegen.context.append_basic_block(self.func, "entry");
        self.head = Some(head);
        return head;
    }
    /// Place a context at the head of this function. If this function has no head, generate an
    /// entry block.
    pub fn to_head(&mut self, codegen: &mut Codegen<'ctx>) {
        let head = self.get_head(codegen);
        codegen.builder.position_at_end(head);
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

/// The default linkage of lambda values
pub const DEFAULT_LAMBDA_LINKAGE: Option<Linkage> = None;

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context
    pub fn new(context: &'ctx Context, module: Module<'ctx>) -> Codegen<'ctx> {
        Codegen {
            consts: HashMap::default(),
            reprs: HashMap::default(),
            counter: 0,
            module,
            builder: context.create_builder(),
            context,
        }
    }
    /// Get the representation for a given type, if any
    pub fn get_repr(&mut self, t: &TypeId) -> Result<Repr<'ctx>, Error> {
        //TODO: caching, `Entry`, etc...
        match t.as_enum() {
            ValueEnum::BoolTy(_) => Ok(Repr::Type(self.context.bool_type().into())),
            ValueEnum::Finite(f) => Ok(self.compile_finite(f)),
            _ => unimplemented!(),
        }
    }
    /// Create a function prototype for a constant pi type
    pub fn const_pi_prototype(&mut self, pi: &Pi) -> Result<Prototype<'ctx>, Error> {
        if pi.depth() != 0 {
            return Err(Error::NotConst);
        }
        let region = pi.def_region();
        let result = pi.result();
        if result.depth() != 0 {
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

        for input_ty in region.iter() {
            match self.get_repr(input_ty)? {
                Repr::Type(t) => {
                    input_ixes.push(input_reprs.len() as isize);
                    input_reprs.push(t);
                }
                Repr::Function(_) => unimplemented!(),
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
            &format!("__lambda_{}", self.counter),
            result_ty,
            DEFAULT_LAMBDA_LINKAGE,
        );
        self.counter += 1;

        // Construct a context, binding local values to types
        let mut ctx = LocalCtx::new(self, region.clone(), result_fn, None);

        // Bind parameters
        for (i, ix) in input_ixes.iter().copied().enumerate() {
            let param = ValId::from(
                region
                    .clone()
                    .param(i)
                    .expect("Iterated index is in bounds"),
            );
            match ix {
                PROP_IX => {
                    ctx.locals.insert(param, Local::Unit);
                }
                EMPTY_IX => {
                    ctx.locals.insert(param, Local::Contradiction);
                }
                IRREP_IX => {
                    ctx.locals.insert(param, Local::Irrep);
                }
                ix => {
                    ctx.locals.insert(
                        param,
                        Local::Value(
                            result_fn
                                .get_nth_param(ix as u32)
                                .expect("Index in vector is in bounds")
                                .into(),
                        ),
                    );
                }
            }
        }

        Ok(Prototype::Ctx(ctx))
    }

    /// Build a return value into a function context
    pub fn build_retv(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        v: &ValId,
    ) -> Result<InstructionValue<'ctx>, Error> {
        ctx.to_head(self);
        let retv = self.build(ctx, v)?;
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
            Local::Unit | Local::Contradiction | Local::Irrep => {
                undef_retv.as_ref().map(|v| v as &dyn BasicValue)
            }
        };
        let ret = self.builder.build_return(retv_borrow);
        Ok(ret)
    }

    /// Compile a constant `rain` lambda function
    pub fn compile_lambda(&mut self, l: &Lambda) -> Result<Const<'ctx>, Error> {
        let ty = l.get_ty();
        let prototype = self.const_pi_prototype(ty.deref())?;

        match prototype {
            Prototype::Ctx(mut ctx) => {
                self.build_retv(&mut ctx, l.result())?;
                Ok(Const::Function(ctx.func))
            }
            Prototype::Prop => Ok(Const::Unit),
            Prototype::Irrep => Ok(Const::Irrep),
        }
    }

    /// Compile a static-constant `rain` function
    pub fn compile_constant(&mut self, _ty: &Pi, _val: &ValId) -> FunctionValue<'ctx> {
        unimplemented!()
    }

    /// Compile a constant logical `rain` function
    pub fn compile_logical(&mut self, l: &Logical) -> FunctionValue<'ctx> {
        match l.arity() {
            1 => match l.data() {
                0b00 => self.compile_constant(&LOGICAL_OP_TYS[0], &true.into()),
                0b01 => unimplemented!(), // logical not
                0b10 => unimplemented!(), // logical identity
                0b11 => self.compile_constant(&LOGICAL_OP_TYS[1], &false.into()),
                _ => unreachable!(),
            },
            _ => unimplemented!(),
        }
    }
    /// Compile a boolean
    pub fn compile_bool(&self, b: bool) -> IntValue<'ctx> {
        self.context.bool_type().const_int(b as u64, false)
    }

    /// Compile a finite
    pub fn compile_finite(&mut self, f: &Finite) -> Repr<'ctx> {
        let value: u128 = f.0;
        if value == 0 {
            Repr::Empty
        } else if value == 1 {
            Repr::Prop
        } else if value == 2 {
            Repr::Type(self.context.bool_type().into())
        } else if value < (1 << 8) {
            Repr::Type(self.context.i8_type().into())
        } else if value < (1 << 16) {
            Repr::Type(self.context.i16_type().into())
        } else if value < (1 << 32) {
            Repr::Type(self.context.i32_type().into())
        } else if value < (1 << 64) {
            Repr::Type(self.context.i64_type().into())
        } else {
            Repr::Type(self.context.i128_type().into())
        }
    }

    /// Compile an index
    pub fn compile_index(&mut self, i: &Index) -> Const<'ctx> {
        let type_bound = (*i.get_ty()).0;
        if type_bound == 0 {
            panic!("Error: Unable to compile index of type Finite(0)");
        }
        let this_value = i.ix();
        if type_bound <= this_value {
            panic!("Index({}) is not a valid instance of Finite({})", 
            this_value, type_bound);
        } else {
            if type_bound == 1 {
                Const::Unit
            } else if type_bound == 2 {
                self.context.bool_type().const_int(this_value as u64, false).into()
            } else if type_bound < (1 << 8) {
                self.context.i8_type().const_int(this_value as u64, false).into()
            } else if type_bound < (1 << 16) {
                self.context.i16_type().const_int(this_value as u64, false).into()
            } else if type_bound < (1 << 32) {
                self.context.i32_type().const_int(this_value as u64, false).into()
            } else if type_bound < (1 << 64) {
                self.context.i64_type().const_int(this_value as u64, false).into()
            } else {
                self.context.i128_type().const_int_arbitrary_precision(
                    &[(this_value >> 64) as u64, this_value as u64]
                ).into()
            }
        }

    }

    /// Get a compiled constant `rain` value or function
    pub fn compile_enum(&mut self, v: &ValueEnum) -> Result<Const<'ctx>, Error> {
        match v {
            ValueEnum::BoolTy(_) => Ok(Const::Unit),
            ValueEnum::Bool(b) => Ok(self.compile_bool(*b).into()),
            ValueEnum::Finite(_) => Ok(Const::Unit),
            ValueEnum::Index(_i) => Ok(self.compile_index(_i)),
            ValueEnum::Lambda(l) => self.compile_lambda(l),
            ValueEnum::Pi(_p) => unimplemented!(),
            ValueEnum::Gamma(_g) => unimplemented!(),
            ValueEnum::Phi(_p) => unimplemented!(),
            ValueEnum::Parameter(_) => Err(Error::NotConst),
            ValueEnum::Product(_p) => unimplemented!(),
            ValueEnum::Tuple(_t) => unimplemented!(),
            ValueEnum::Sexpr(_s) => unimplemented!(),
            ValueEnum::Universe(_) => Ok(Const::Unit),
            ValueEnum::Logical(l) => Ok(self.compile_logical(l).into()),
        }
    }

    /// Get a compiled constant `rain` value
    pub fn compile_const(&mut self, v: &ValId) -> Result<Const<'ctx>, Error> {
        if let Some(c) = self.consts.get(v) {
            Ok(*c)
        } else {
            let c = self.compile_enum(v.as_enum())?;
            self.consts.insert(v.clone(), c);
            Ok(c)
        }
    }

    /// Build a parameter in a local context
    pub fn build_parameter(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        p: &Parameter,
    ) -> Result<Local<'ctx>, Error> {
        ctx.locals
            .get(&ValId::from(p.clone()))
            .cloned()
            .ok_or(Error::InternalError(
                "Context should have parameters pre-registered!",
            ))
    }

    /// Build a tuple in a local context
    pub fn build_tuple(
        &mut self,
        _ctx: &mut LocalCtx<'ctx>,
        _p: &Tuple,
    ) -> Result<Local<'ctx>, Error> {
        unimplemented!()
    }

    /// Build the evaluation of a logical operation on an argument list
    pub fn build_logical_expr(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        l: Logical,
        args: &[ValId],
    ) -> Result<Local<'ctx>, Error> {
        // Arity check
        let l_arity = l.arity() as usize;
        debug_assert!(
            l_arity >= args.len(),
            "Arity (({}).arity() = {}) must be greater or equal to than the length of the argument list ({:?}.len() = {})",
            l, l_arity, args, args.len()
        );
        // Partial logical evaluation check
        if l_arity != args.len() {
            unimplemented!()
        }
        // Direct construction of constant operations
        if let Some(c) = l.get_const() {
            return Ok(self.compile_bool(c).into());
        }
        // Direct construction of non-constant operations
        match l_arity {
            0 => panic!("Zero arity logical operations ({}) are invalid!", l),
            // Unary operations
            1 => {
                let arg = self.build(ctx, &args[0])?;
                if l == logical::Not {
                    let arg: IntValue = arg.try_into().expect("A boolean value");
                    return Ok(self.builder.build_not(arg, "pnot").into());
                }
                if l == logical::Id {
                    return Ok(arg);
                }
                panic!("Invalid non-constant unary operation!")
            }
            // Binary operations
            2 => {
                if l == logical::And {
                    let lhs: IntValue = self
                        .build(ctx, &args[0])?
                        .try_into()
                        .expect("A boolean value");
                    let rhs: IntValue = self
                        .build(ctx, &args[1])?
                        .try_into()
                        .expect("A boolean value");
                    return Ok(self.builder.build_and(lhs, rhs, "pand").into());
                }
                if l == logical::Or {
                    let lhs: IntValue = self
                        .build(ctx, &args[0])?
                        .try_into()
                        .expect("A boolean value");
                    let rhs: IntValue = self
                        .build(ctx, &args[1])?
                        .try_into()
                        .expect("A boolean value");
                    return Ok(self.builder.build_or(lhs, rhs, "por").into());
                }
                if l == logical::Xor {
                    let lhs: IntValue = self
                        .build(ctx, &args[0])?
                        .try_into()
                        .expect("A boolean value");
                    let rhs: IntValue = self
                        .build(ctx, &args[1])?
                        .try_into()
                        .expect("A boolean value");
                    return Ok(self.builder.build_xor(lhs, rhs, "pxor").into());
                }
                // Go to general strategy: split and evaluate
            }
            _ => {} // Go to general strategy: split and evaluate
        }
        // General strategy: split and evaluate
        let true_branch = l.apply(true);
        let false_branch = l.apply(false);
        let select = self
            .build(ctx, &args[0])?
            .try_into()
            .expect("A boolean value");
        let (high, low) = match (true_branch, false_branch) {
            (Either::Left(high), Either::Left(low)) => {
                // Selection between constant booleans: arity 1!
                debug_assert_eq!(l_arity, 1);
                (self.compile_bool(high), self.compile_bool(low))
            }
            (Either::Right(high), Either::Right(low)) => {
                // Selection between function results: arity > 1
                debug_assert!(l_arity > 1);
                let high: IntValue = self
                    .build_logical_expr(ctx, high, &args[1..])?
                    .try_into()
                    .expect("A boolean value");
                let low: IntValue = self
                    .build_logical_expr(ctx, low, &args[1..])?
                    .try_into()
                    .expect("A boolean value");
                (high, low)
            }
            (t, f) => panic!("Branches {}, {} of {} should have the same arity!", t, f, l),
        };
        let is_high = self.builder.build_and(high, select, "is_high");
        let not_select = self.builder.build_not(select, "nsel");
        let is_low = self.builder.build_and(low, not_select, "is_low");
        Ok(self.builder.build_or(is_high, is_low, "psplit").into())
    }

    /// Build an S-expression in a local context
    pub fn build_sexpr(
        &mut self,
        ctx: &mut LocalCtx<'ctx>, //TODO: this...
        s: &Sexpr,
    ) -> Result<Local<'ctx>, Error> {
        if s.len() == 0 {
            return Ok(Local::Unit);
        }
        match s[0].as_enum() {
            // Special case logical operation building
            ValueEnum::Logical(l) => return self.build_logical_expr(ctx, *l, &s.as_slice()[1..]),
            _ => {}
        }
        //TODO: build s[0], etc...
        unimplemented!()
    }

    /// Build a `ValueEnum` in a local context
    pub fn build_enum(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        v: &ValueEnum,
    ) -> Result<Local<'ctx>, Error> {
        match v {
            v @ ValueEnum::BoolTy(_)
            | v @ ValueEnum::Bool(_)
            | v @ ValueEnum::Finite(_)
            | v @ ValueEnum::Index(_)
            | v @ ValueEnum::Logical(_)
            | v @ ValueEnum::Universe(_) => self.compile_enum(v).map(Local::from),
            ValueEnum::Lambda(_l) => unimplemented!(),
            ValueEnum::Pi(_p) => unimplemented!(),
            ValueEnum::Gamma(_g) => unimplemented!(),
            ValueEnum::Phi(_p) => unimplemented!(),
            ValueEnum::Parameter(p) => self.build_parameter(ctx, p),
            ValueEnum::Product(_) => unimplemented!(),
            ValueEnum::Tuple(t) => self.build_tuple(ctx, t),
            ValueEnum::Sexpr(s) => self.build_sexpr(ctx, s),
        }
    }

    /// Build a `ValId` in a local context
    pub fn build(&mut self, ctx: &mut LocalCtx<'ctx>, v: &ValId) -> Result<Local<'ctx>, Error> {
        // NOTE: compiling a constant never leaves the builder, and we do not implemented nested regions currently.
        // Hence, we work with the (bad) assumption that we never have to worry about the builder jumping around because
        // any serialization respecting dependency order is a valid serialization. Note we *also* ignore lifetime order,
        // but we'll fix that when we implement DFS
        if ctx.region.depth() > 1 {
            return Err(Error::NotImplemented("Nested region ValId compilation"));
        }
        // Constant regions are constants
        if v.depth() == 0 {
            return self.compile_const(v).map(Local::from);
        } else if let Some(l) = ctx.locals.get(v) {
            // Check the local cache
            return Ok(*l);
        }
        let result = self.build_enum(ctx, v.as_enum())?;
        ctx.locals.insert(v.clone(), result);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::execution_engine::JitFunction;
    use inkwell::OptimizationLevel;
    use rain_lang::parser::builder::Builder;

    #[test]
    fn identity_lambda_compiles_properly() {
        // Setup
        let mut builder = Builder::<&str>::new();
        let context = Context::create();
        let module = context.create_module("identity_bool");
        let execution_engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .unwrap();
        let mut codegen = Codegen::new(&context, module);

        // ValId construction
        let (rest, id) = builder.parse_expr("|x: #bool| x").expect("Valid lambda");
        assert_eq!(rest, "");

        // Codegen
        let f = match codegen.compile_const(&id).expect("Valid constant") {
            Const::Function(f) => f,
            r => panic!("Invalid constant generated: {:?}", r),
        };

        let f_name = f
            .get_name()
            .to_str()
            .expect("Generated name must be valid UTF-8");
        assert_eq!(f_name, "__lambda_0");

        // Jit
        let jit_f: JitFunction<unsafe extern "C" fn(bool) -> bool> =
            unsafe { execution_engine.get_function(f_name) }.expect("Valid IR generated");

        // Run
        for x in [true, false].iter() {
            unsafe {
                assert_eq!(jit_f.call(*x), *x);
            }
        }
    }

    #[test]
    fn mux_lambda_compiles_properly() {
        // Setup
        let mut builder = Builder::<&str>::new();
        let context = Context::create();
        let module = context.create_module("mux");
        let execution_engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .unwrap();
        let mut codegen = Codegen::new(&context, module);

        // ValId construction
        let mux_p = "|select: #bool high: #bool low: #bool| (#or (#and select high) (#and (#not select) low))";
        let (rest, mux) = builder.parse_expr(mux_p).expect("Valid lambda");
        assert_eq!(rest, "");

        // Codegen
        let f = match codegen.compile_const(&mux).expect("Valid constant") {
            Const::Function(f) => f,
            r => panic!("Invalid constant generated: {:?}", r),
        };

        let f_name = f
            .get_name()
            .to_str()
            .expect("Generated name must be valid UTF-8");
        assert_eq!(f_name, "__lambda_0");

        // Jit
        let jit_f: JitFunction<unsafe extern "C" fn(bool, bool, bool) -> bool> =
            unsafe { execution_engine.get_function(f_name) }.expect("Valid IR generated");

        // Run
        for select in [true, false].iter().copied() {
            for high in [true, false].iter().copied() {
                for low in [true, false].iter().copied() {
                    unsafe {
                        assert_eq!(
                            jit_f.call(select, high, low),
                            if select { high } else { low },
                            "Invalid result for select = {}, high = {}, low = {}",
                            select,
                            high,
                            low
                        )
                    }
                }
            }
        }
    }

    #[test]
    fn identity_finite_and_index_compiles_properly() {
        // Setup
        let mut builder = Builder::<&str>::new();
        let context = Context::create();
        let module = context.create_module("identity_bool");
        let execution_engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .unwrap();
        let mut codegen = Codegen::new(&context, module);

        // ValId construction
        let (rest, id) = builder.parse_expr("|x: #finite(6)| x").expect("Valid lambda");
        assert_eq!(rest, "");

        // Codegen
        let f = match codegen.compile_const(&id).expect("Valid constant") {
            Const::Function(f) => f,
            r => panic!("Invalid constant generated: {:?}", r),
        };

        let f_name = f
            .get_name()
            .to_str()
            .expect("Generated name must be valid UTF-8");
        assert_eq!(f_name, "__lambda_0");

        let (rest, id) = builder.parse_expr("#ix(6)[4]").expect("Valid Index Instance");
        assert_eq!(rest, "");

        let val = match codegen.compile_const(&id).expect("Valid Constant") {
            Const::Value(i) => i,
            r => panic!("Invalid constant generated {:?}", r),
        };
        let int_val = match val {
            BasicValueEnum::IntValue(i) => i,
            _ => panic!("Wrong type: expect u8 for ix(6)[4]"),
        };
        assert_eq!(int_val.get_type().get_bit_width(), 8);

        let (rest, id) = builder.parse_expr("#ix(512)[4]").expect("Valid Index Instance");
        assert_eq!(rest, "");

        let val = match codegen.compile_const(&id).expect("Valid Constant") {
            Const::Value(i) => i,
            r => panic!("Invalid constant generated {:?}", r),
        };
        let int_val = match val {
            BasicValueEnum::IntValue(i) => i,
            _ => panic!("Wrong type: expect u16 for ix(512)[4]"),
        };
        assert_eq!(int_val.get_type().get_bit_width(), 16);
        
        // Jit
        let jit_f: JitFunction<unsafe extern "C" fn(u8) -> u8> =
            unsafe { execution_engine.get_function(f_name) }.expect("Valid IR generated");

        // Run
        unsafe {
            assert_eq!(jit_f.call(4 as u8), 4);
        }
    }
}

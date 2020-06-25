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
use inkwell::types::{BasicType, BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{
    AnyValueEnum, BasicValue, BasicValueEnum, FunctionValue, 
    InstructionValue, IntValue,
};
use inkwell::AddressSpace;
use rain_ir::function::{lambda::Lambda, pi::Pi};
use rain_ir::primitive::finite::{Finite, Index};
use rain_ir::primitive::logical::{self, Logical, LOGICAL_OP_TYS};
use rain_ir::region::Regional;
use rain_ir::region::{Parameter, Region};
use rain_ir::typing::Typed;
use rain_ir::value::{
    expr::Sexpr,
    tuple::{Product, Tuple},
    TypeId, ValId, ValueEnum,
};
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
A representation of product
*/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRepr<'ctx> {
    /// A mapping since we need to skip Repr::Unit
    /// mapping[i] hold the position of ith element in the struct
    pub mapping: Vec<Option<u32>>,
    /// The actual representation
    pub repr: StructType<'ctx>,
}

/**
A representation for a `rain` type
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
            ValueEnum::Product(p) => self.compile_product(p),
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
            Repr::Product(p) => p.repr.into(),
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
                Repr::Product(p) => {
                    input_ixes.push(input_reprs.len() as isize);
                    input_reprs.push(p.repr.into());
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
            panic!(
                "Index({}) is not a valid instance of Finite({})",
                this_value, type_bound
            );
        } else {
            if type_bound == 1 {
                Const::Unit
            } else if type_bound == 2 {
                self.context
                    .bool_type()
                    .const_int(this_value as u64, false)
                    .into()
            } else if type_bound < (1 << 8) {
                self.context
                    .i8_type()
                    .const_int(this_value as u64, false)
                    .into()
            } else if type_bound < (1 << 16) {
                self.context
                    .i16_type()
                    .const_int(this_value as u64, false)
                    .into()
            } else if type_bound < (1 << 32) {
                self.context
                    .i32_type()
                    .const_int(this_value as u64, false)
                    .into()
            } else if type_bound < (1 << 64) {
                self.context
                    .i64_type()
                    .const_int(this_value as u64, false)
                    .into()
            } else {
                self.context
                    .i128_type()
                    .const_int_arbitrary_precision(&[(this_value >> 64) as u64, this_value as u64])
                    .into()
            }
        }
    }

    /// Compile a product
    pub fn compile_product(&mut self, p: &Product) -> Result<Repr<'ctx>, Error> {
        let mut mapping: Vec<Option<u32>> = Vec::new();
        let mut struct_index = 0;
        let mut repr_vec: Vec<BasicTypeEnum<'ctx>> = Vec::new();
        let mut reprs = p.iter().map(|ty| self.get_repr(ty));
        while let Some(repr) = reprs.next() {
            let repr = repr?;
            match repr {
                Repr::Type(ty) => {
                    repr_vec.push(ty);
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
                Repr::Function(f) => {
                    repr_vec.push(f.ptr_type(AddressSpace::Global).into());
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
                Repr::Empty => return Ok(Repr::Empty),
                Repr::Irrep => {
                    let mut return_empty = false;
                    while let Some(r) = reprs.next() {
                        if r? == Repr::Empty {
                            return_empty = true;
                        }
                    }
                    if return_empty {
                        return Ok(Repr::Empty);
                    }
                }
                Repr::Prop => mapping.push(None),
                Repr::Product(p) => {
                    repr_vec.push(p.repr.into());
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
            }
        }
        if struct_index == 0 {
            Ok(Repr::Empty)
        } else {
            let repr = self.context.struct_type(&repr_vec[..], false);
            Ok(Repr::Product(ProductRepr { mapping, repr }))
        }
    }

    /// Compile a tuple
    // pub fn compile_tuple(&mut self, t: &Tuple) -> Result<Const<'ctx>, Error> {
    //     // let p = t.ty().clone_ty().as_enum();
    //     // match p {
    //     //     ValueEnum::Product(product) => {
    //     //         let repr = match self.compile_product(product)? {
    //     //             Repr::Product(tmp) => tmp,
    //     //             _ => {return Err(Error::InternalError("Expect a product"))}
    //     //         };
    //     //         let values: Vec<BasicValueEnum<'ctx>> = Vec::new();
    //     //         for (i, mapped) in repr.mapping.iter().enumerate() {
    //     //             if let Some(mapped_pos) = mapped {
    //     //                 let this_type = repr.repr.get_field_type_at_index()
    //     //             }
    //     //         }
    //     //     },
    //     //     _ => {return Err(Error::InternalError("Expected a product"))}
    //     // };

    //     // unimplemented!();
    // }

    /// Get a compiled constant `rain` value or function
    pub fn compile_enum(&mut self, v: &ValueEnum) -> Result<Const<'ctx>, Error> {
        match v {
            ValueEnum::BoolTy(_) => Ok(Const::Unit),
            ValueEnum::Bool(b) => Ok(self.compile_bool(*b).into()),
            ValueEnum::Finite(_) => Ok(Const::Unit),
            ValueEnum::Index(_i) => Ok(self.compile_index(_i)),
            ValueEnum::Lambda(l) => self.compile_lambda(l),
            ValueEnum::Pi(p) => unimplemented!("Pi compilation: {}", p),
            ValueEnum::Gamma(g) => unimplemented!("Gamma compilation: {}", g),
            ValueEnum::Phi(p) => unimplemented!("Phi compilation: {}", p),
            ValueEnum::Parameter(_) => Err(Error::NotConst),
            ValueEnum::Product(p) => unimplemented!("Product compilation: {}", p),
            ValueEnum::Tuple(t) => unimplemented!("Tuple compilation: {}", t),
            ValueEnum::Sexpr(s) => unimplemented!("Sexpr compilation: {}", s),
            ValueEnum::Universe(_) => Ok(Const::Unit),
            ValueEnum::Logical(l) => Ok(self.compile_logical(l).into()),
            ValueEnum::Cast(c) => unimplemented!("Cast compilation: {}", c),
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
        ctx: &mut LocalCtx<'ctx>,
        p: &Tuple,
    ) -> Result<Local<'ctx>, Error> {
        let p_enum = p.ty().as_enum();
        match p_enum {
            ValueEnum::Product(product) => {
                let repr = match self.compile_product(product)? {
                    Repr::Product(tmp) => tmp,
                    Repr::Prop => return Ok(Local::Unit),
                    Repr::Empty => return Ok(Local::Contradiction),
                    // TODO: think about Local::Irrep
                    Repr::Irrep => return Err(Error::Irrepresentable),
                    // TODO: Rethink the following later
                    Repr::Function(_f) => {
                        return Err(Error::NotImplemented("Function in tuple not implemented"));
                    }
                    Repr::Type(_t) => {
                        return Err(Error::NotImplemented("Type in tuple not supported yet"))
                    }
                };
                let mut values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, mapped) in repr.mapping.iter().enumerate() {
                    if let Some(_mapped_pos) = mapped {
                        let this_result = self.build(ctx, &p[i])?;
                        // Note: This assumes that each type has unique representation
                        let value: BasicValueEnum<'ctx> = match this_result {
                            Local::Value(v) => match v.try_into() {
                                Ok(v) => v,
                                Err(()) => unimplemented!("Function types in tuple"),
                            },
                            l => panic!("Invalid struct member {:?}", l),
                        };
                        values.push(value);
                    }
                }
                Ok(Local::Value(
                    repr.repr.const_named_struct(&values[..]).into(),
                ))
            }
            _ => Err(Error::InternalError("Expected a product")),
        }
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

    /// Build a function call with arguments
    pub fn build_function_call(
        &mut self, 
        ctx: &mut LocalCtx<'ctx>,
        f: &FunctionValue<'ctx>,
        args: &[ValId]
    ) -> Result<Local<'ctx>, Error> {
        let mut this_args: Vec<BasicValueEnum> = Vec::new();
        for arg in args {
            match self.build(ctx, arg)? {
                Local::Contradiction => panic!("Internal Error: function argument is a contradiction"),
                Local::Irrep => panic!("Internal Error: function argument unrepresentable"),
                Local::Unit => { 
                    return Ok(Local::Unit); 
                },
                Local::Value(v) => {
                    match v.try_into() {
                        Ok(this_v) => this_args.push(this_v),
                        Err(_) => panic!("Internal Error, function argument is not a basic value")
                    };
                }
            }
        }
        match self.builder
            .build_call::<FunctionValue<'ctx>>(*f, &this_args[..], "f")
            .try_as_basic_value().left() {
                Some(b) => Ok(b.into()),
                None => Ok(Local::Unit)
            }
    }

    /// Build a function application in a local context
    pub fn build_app(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        f: &ValId,
        args: &[ValId],
    ) -> Result<Local<'ctx>, Error> {
        if args.len() == 0 {
            return self.build(ctx, f);
        }
        match f.as_enum() {
            // Special case logical operation building
            ValueEnum::Logical(l) => return self.build_logical_expr(ctx, *l, args),
            _ => {}
        }

        let ty = f.ty();
        let f_code: BasicValueEnum = match self.build(ctx, f)? {
            Local::Value(v) => v.try_into().expect("Unimplemented"),
            spec_repr => return Ok(spec_repr),
        };

        match ty.as_enum() {
            ValueEnum::Product(_p) => {
                match self.get_repr(&ty.clone_ty())? {
                    Repr::Prop => Ok(Local::Unit),
                    Repr::Empty => Ok(Local::Contradiction),
                    Repr::Irrep => Ok(Local::Irrep),
                    Repr::Type(_t) => unimplemented!(),
                    Repr::Function(_f) => unimplemented!(),
                    Repr::Product(p) => {
                        // Generate GEP.
                        if args.len() != 1 {
                            unimplemented!();
                        }
                        let ix = match args[0].as_enum() {
                            ValueEnum::Index(ix) => ix.ix() as usize,
                            _ => unimplemented!(),
                        };
                        let repr_ix = if let Some(ix) = p.mapping[ix] {
                            ix
                        } else {
                            return Ok(Local::Unit);
                        };
                        let struct_value = match f_code {
                            BasicValueEnum::StructValue(s) => s,
                            _ => panic!("Internal error: Repr::Product guarantees BasicValueEnum::StructValue")
                        };
                        let element = self
                            .builder
                            .build_extract_value(struct_value, repr_ix, "idx")
                            .expect("Internal error: valid index guaranteed by IR construction");
                        Ok(Local::Value(element.into()))
                    }
                }
            }
            ValueEnum::Lambda(l) => {
                let compiled_lambda = match self.compile_lambda(l) {
                    Ok(res) => {
                        match res {
                            Const::Function(f) => f,
                            _ => panic!("Expected function, got something else")
                        }
                    },
                    Err(_) => unimplemented!("Non-constant lambda not implemented")
                };
                Ok(self.build_function_call(ctx, &compiled_lambda, args)?.into())
            }
            _ => unimplemented!(),
        }
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
        self.build_app(ctx, &s[0], &s.as_slice()[1..])
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
            ValueEnum::Lambda(l) => unimplemented!("Lambda building: {}", l),
            ValueEnum::Pi(p) => unimplemented!("Pi building: {}", p),
            ValueEnum::Gamma(g) => unimplemented!("Gamma building: {}", g),
            ValueEnum::Phi(p) => unimplemented!("Phi building: {}", p),
            ValueEnum::Parameter(p) => self.build_parameter(ctx, p),
            ValueEnum::Product(p) => unimplemented!("Product building: {}", p),
            ValueEnum::Tuple(t) => self.build_tuple(ctx, t),
            ValueEnum::Sexpr(s) => self.build_sexpr(ctx, s),
            ValueEnum::Cast(c) => unimplemented!("Cast building: {}", c),
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
    use rain_ir::parser::builder::Builder;

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
        let (rest, id) = builder
            .parse_expr("|x: #finite(6)| x")
            .expect("Valid lambda");
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

        let (rest, id) = builder
            .parse_expr("#ix(6)[4]")
            .expect("Valid Index Instance");
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

        let (rest, id) = builder
            .parse_expr("#ix(512)[4]")
            .expect("Valid Index Instance");
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

    #[test]
    fn identity_product_compiles_properly() {
        // Setup
        let mut builder = Builder::<&str>::new();
        let context = Context::create();
        let module = context.create_module("identity_bool");
        // let execution_engine = module
        //     .create_jit_execution_engine(OptimizationLevel::None)
        //     .unwrap();
        let mut codegen = Codegen::new(&context, module);

        // ValId construction
        let (rest, id) = builder
            .parse_expr("|x: #product[#finite(73) #finite(1025)]| x")
            .expect("Valid lambda");
        assert_eq!(rest, "");

        // Codegen
        let f = match codegen.compile_const(&id).expect("Valid constant") {
            Const::Function(f) => f,
            r => panic!("Invalid constant generated: {:?}", r),
        };

        f.print_to_stderr();

        let f_name = f
            .get_name()
            .to_str()
            .expect("Generated name must be valid UTF-8");
        assert_eq!(f_name, "__lambda_0");

        // #[repr(C)]
        // #[derive(Debug, Copy, Clone, PartialEq)]
        // struct _Product0 {
        //     first: i8,
        //     second: i16
        // }

        // Jit
        // let jit_f: JitFunction<unsafe extern "C" fn(_Product0) -> _Product0> =
        //     unsafe { execution_engine.get_function(f_name) }.expect("Valid IR generated");

        // // Run
        // for first in 0..10 {
        //     for second in 0..10 {
        //         let tuple = _Product0{first, second};
        //         unsafe {
        //             assert_eq!(
        //                 jit_f.call(tuple),
        //                 tuple
        //             );
        //         }
        //     }
        // }
    }

    #[test]
    fn projections_compile_correctly() {}
}

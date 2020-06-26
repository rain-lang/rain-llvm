/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]

pub mod codegen;
pub mod error;
pub mod repr;

/*
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
    
    /// Build a function call with arguments
    pub fn build_function_call(
        &mut self,
        ctx: &mut LocalCtx<'ctx>,
        f: &FunctionValue<'ctx>,
        args: &[ValId],
    ) -> Result<Local<'ctx>, Error> {
        let mut this_args: Vec<BasicValueEnum> = Vec::new();
        for arg in args {
            match self.build(ctx, arg)? {
                Local::Contradiction => return Ok(Local::Contradiction),
                Local::Irrep => return Err(Error::Irrepresentable),
                Local::Unit => {
                    return Ok(Local::Unit);
                }
                Local::Value(v) => {
                    match v.try_into() {
                        Ok(this_v) => this_args.push(this_v),
                        Err(_) => unimplemented!("Higher order functions not implemented"),
                    };
                }
            }
        }
        match self
            .builder
            .build_call::<FunctionValue<'ctx>>(*f, &this_args[..], "call")
            .try_as_basic_value()
            .left()
        {
            Some(b) => Ok(b.into()),
            None => Ok(Local::Unit),
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
                    Ok(res) => match res {
                        Const::Function(f) => f,
                        _ => panic!("Expected function, got something else"),
                    },
                    Err(_) => unimplemented!("Non-constant lambda not implemented"),
                };
                Ok(self
                    .build_function_call(ctx, &compiled_lambda, args)?
                    .into())
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
*/

/*
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
}
*/
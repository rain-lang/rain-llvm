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
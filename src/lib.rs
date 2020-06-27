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

*/

/*
#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::execution_engine::JitFunction;
    use inkwell::OptimizationLevel;
    use rain_ir::parser::builder::Builder;

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
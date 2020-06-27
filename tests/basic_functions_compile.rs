use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::values::FunctionValue;
use inkwell::OptimizationLevel;
use rain_ir::parser::builder::Builder;
use rain_llvm::codegen::Codegen;
use std::convert::TryInto;

#[test]
fn boolean_identity_compiles() {
    // Setup
    let mut builder = Builder::<&str>::new();
    let context = Context::create();
    let module = context.create_module("identity_bool");
    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    let mut codegen = Codegen::new(&context, module);

    // ValId construction
    let (rest, bool_id) = builder.parse_expr("|x: #bool| x").expect("Valid function");
    assert_eq!(rest, "");

    // Codegen
    let f: FunctionValue = codegen
        .build(&bool_id)
        .expect("Compilation works")
        .try_into()
        .expect("Compiles to a function");

    // "Linking"
    let f_name = f
        .get_name()
        .to_str()
        .expect("Generated name must be valid UTF-8");

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
    let mux_p =
        "|select: #bool high: #bool low: #bool| (#or (#and select high) (#and (#not select) low))";
    let (rest, mux) = builder.parse_expr(mux_p).expect("Valid lambda");
    assert_eq!(rest, "");

    // Codegen
    let f: FunctionValue = codegen
        .build(&mux)
        .expect("Compilation works")
        .try_into()
        .expect("Compiles to a function");

    // "Linking"
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

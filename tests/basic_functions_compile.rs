use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::values::FunctionValue;
use inkwell::OptimizationLevel;
use rain_ir::parser::builder::Builder;
use rain_llvm::codegen::Codegen;
use std::convert::TryInto;

#[test]
fn boolean_identity_compiles() {
    let mut builder = Builder::<&str>::new();
    let context = Context::create();
    let module = context.create_module("identity_bool");
    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let (rest, bool_id) = builder.parse_expr("|x: #bool| x").expect("Valid function");
    assert_eq!(rest, "");
    let f: FunctionValue = codegen
        .build(&bool_id)
        .expect("Compilation works")
        .try_into()
        .expect("Compiles to a function");

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

use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::values::{FunctionValue, IntValue};
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
    let f: FunctionValue = codegen
        .build(&id)
        .expect("Compilation works")
        .try_into()
        .expect("Compiles to a function");

    let f_name = f
        .get_name()
        .to_str()
        .expect("Generated name must be valid UTF-8");
    assert_eq!(f_name, "__lambda_0");

    let (rest, ix) = builder
        .parse_expr("#ix(6)[4]")
        .expect("Valid Index Instance");
    assert_eq!(rest, "");

    let val: IntValue = codegen.build(&ix).expect("Valid value").try_into().expect("Integer value");
    assert_eq!(val.get_type().get_bit_width(), 8);

    let (rest, ix) = builder
        .parse_expr("#ix(512)[4]")
        .expect("Valid Index Instance");
    assert_eq!(rest, "");

    let val: IntValue = codegen.build(&ix).expect("Valid value").try_into().expect("Integer value");
    assert_eq!(val.get_type().get_bit_width(), 16);

    // Jit
    let jit_f: JitFunction<unsafe extern "C" fn(u8) -> u8> =
        unsafe { execution_engine.get_function(f_name) }.expect("Valid IR generated");

    // Run
    for ix in 0..5 {
        unsafe {
            assert_eq!(jit_f.call(ix as u8), ix);
        }
    }
}


#[test]
fn identity_product_compiles_properly() {
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
        .parse_expr("|x: #product[#finite(73) #finite(1025)]| x")
        .expect("Valid lambda");
    assert_eq!(rest, "");

    // Codegen
    let f: FunctionValue = codegen
        .build(&id)
        .expect("Compilation works")
        .try_into()
        .expect("Compiles functions");
    //f.print_to_stderr();

    let _f_name = f
        .get_name()
        .to_str()
        .expect("Generated name must be valid UTF-8");

    let f_shim = codegen.build_shim(f, "shim", None);
    let f_shim_name = f_shim
        .get_name()
        .to_str()
        .expect("Generated name must be valid UTF-8");
    
    #[repr(C)]
    #[derive(Debug, Copy, Clone, PartialEq)]
    struct _Product0 {
        first: i8,
        second: i16
    }

    // Jit
    let jit_f: JitFunction<unsafe extern "C" fn(*mut _Product0, *mut _Product0) -> i32> =
        unsafe { execution_engine.get_function(f_shim_name) }.expect("Valid IR generated");

    // Run
    for first in 0..10 {
        for second in 0..10 {
            let mut tuple = _Product0{
                first, second
            };
            let ptr = &mut tuple;
            let mut result = _Product0{
                first: 10, 
                second: 100,
            };
            let result_ptr = &mut result;
            unsafe {
                    let ret_val = jit_f.call(ptr, result_ptr);
                    assert_eq!(ret_val, 0);
                    assert_eq!(result.first, first);
                    assert_eq!(result.second, second);
            }
        }
    }
}

// #[test]
// TODO: Waiting for bug to be fixed in rain side.
// fn index_on_tuple_properly() {
//     // Setup
//     let mut builder = Builder::<&str>::new();
//     let context = Context::create();
//     let module = context.create_module("identity_bool");
//     let execution_engine = module
//         .create_jit_execution_engine(OptimizationLevel::None)
//         .unwrap();
//     let mut codegen = Codegen::new(&context, module);

//     // ValId construction
//     let (rest, id) = builder
//         .parse_expr("|x: #product[#bool #bool]| (x #ix(2)[0])")
//         .expect("Valid lambda");
//     assert_eq!(rest, "");

//     // Codegen
//     let f = match codegen.compile_const(&id).expect("Valid constant") {
//         Const::Function(f) => f,
//         r => panic!("Invalid constant generated: {:?}", r),
//     };

//     // f.print_to_stderr();

//     let f_name = f
//         .get_name()
//         .to_str()
//         .expect("Generated name must be valid UTF-8");
//     assert_eq!(f_name, "__lambda_0");

//     let shim_f = codegen.get_shim(f);

//     shim_f.print_to_stderr();
//     let shim_f_name = shim_f
//         .get_name()
//         .to_str()
//         .expect("Generated name must be valid UTF-8");
//     assert_eq!(shim_f_name, "__lambda_1");

//     #[repr(C)]
//     #[derive(Debug, Copy, Clone, PartialEq)]
//     struct _Product0 {
//         first: bool,
//         second: bool
//     }

//     // Jit
//     let jit_f: JitFunction<unsafe extern "C" fn(*mut _Product0) -> bool> =
//         unsafe { execution_engine.get_function(shim_f_name) }.expect("Valid IR generated");

//     // Run
//     for first in [true, false].iter() {
//         for second in [true, false].iter() {
//             let mut tuple = _Product0{first: *first, second: *second};
//             let ptr = &mut tuple;
//             unsafe {
//                 let ret_val = jit_f.call(ptr);
//                 assert_eq!(ret_val, *first);
//             }
//         }
//     }
// }

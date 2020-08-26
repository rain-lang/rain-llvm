use inkwell::context::Context;
use inkwell::execution_engine::JitFunction;
use inkwell::values::{FunctionValue, IntValue};
use inkwell::OptimizationLevel;
use rain_builder::Builder;
use rain_ir::control::ternary::Ternary;
use rain_ir::primitive::bits::{Add, BitsTy, Mul, Neg};
use rain_ir::value::{ValId, Value};
use rain_llvm::codegen::Codegen;
use rain_llvm::repr::Val;
use std::convert::Into;
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

    let val: IntValue = codegen
        .build(&ix)
        .expect("Valid value")
        .try_into()
        .expect("Integer value");
    assert_eq!(val.get_type().get_bit_width(), 8);

    let (rest, ix) = builder
        .parse_expr("#ix(512)[4]")
        .expect("Valid Index Instance");
    assert_eq!(rest, "");

    let val: IntValue = codegen
        .build(&ix)
        .expect("Valid value")
        .try_into()
        .expect("Integer value");
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
        second: i16,
    }

    // Jit
    let jit_f: JitFunction<unsafe extern "C" fn(*mut _Product0, *mut _Product0) -> i32> =
        unsafe { execution_engine.get_function(f_shim_name) }.expect("Valid IR generated");

    // Run
    for first in 0..10 {
        for second in 0..10 {
            let mut tuple = _Product0 { first, second };
            let ptr = &mut tuple;
            let mut result = _Product0 {
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

#[test]
fn ternary_not() {
    let context = Context::create();
    let module = context.create_module("identity_bool");
    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let t = Ternary::conditional(false.into(), true.into()).unwrap();
    let f: FunctionValue = codegen
        .build(&t.into())
        .expect("Compilation works")
        .try_into()
        .expect("Compiles functions");

    // f.print_to_stderr();
    let _f_name = f
        .get_name()
        .to_str()
        .expect("Generated name must be valid UTF-8");

    // Jit
    let jit_f: JitFunction<unsafe extern "C" fn(b: bool) -> bool> =
        unsafe { execution_engine.get_function(_f_name) }.expect("Valid IR generated");

    // Run
    unsafe {
        assert_eq!(jit_f.call(false), true);
        assert_eq!(jit_f.call(true), false);
    }
}

#[test]
fn bits_compile() {
    let context = Context::create();
    let module = context.create_module("bits");
    // let execution_engine = module
    //     .create_jit_execution_engine(OptimizationLevel::None)
    //     .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let t = BitsTy(3).data(1).unwrap();
    let i: IntValue = codegen
        .build(&t.into())
        .expect("Compilation works")
        .try_into()
        .expect("Compiles values");

    assert_eq!(i.get_type().get_bit_width(), 8);

    let t = BitsTy(1).data(1).unwrap();
    let i: IntValue = codegen
        .build(&t.into())
        .expect("Compilation works")
        .try_into()
        .expect("Compiles values");

    assert_eq!(i.get_type().get_bit_width(), 1);

    let t = BitsTy(14).data(8848).unwrap();
    let i: IntValue = codegen
        .build(&t.into())
        .expect("Compilation works")
        .try_into()
        .expect("Compiles values");

    assert_eq!(i.get_type().get_bit_width(), 16);

    // i.print_to_stderr();
    // let _f_name = f
    //     .get_name()
    //     .to_str()
    //     .expect("Generated name must be valid UTF-8");

    // // Jit
    // let jit_f: JitFunction<unsafe extern "C" fn(b: bool) -> bool> =
    //     unsafe { execution_engine.get_function(_f_name) }.expect("Valid IR generated");

    // // Run
    // unsafe {
    //     assert_eq!(jit_f.call(false), true);
    //     assert_eq!(jit_f.call(true), false);
    // }
}

#[test]
fn bits_add() {
    let context = Context::create();
    let module = context.create_module("bits");
    // let execution_engine = module
    //     .create_jit_execution_engine(OptimizationLevel::None)
    //     .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let t1 = BitsTy(8).data(1).unwrap();
    // let i1: IntValue = codegen.build(&t1.into())
    // .expect("Compilation works")
    // .try_into()
    // .expect("Compiles values");

    let t2 = BitsTy(8).data(2).unwrap();
    // let i2: IntValue = codegen.build(&t2.into())
    // .expect("Compilation works")
    // .try_into()
    // .expect("Compiles values");

    let add_struct = Add::new(8).into_var();

    let arg_vec: Vec<ValId> = vec![t1.into(), t2.into()];
    let app_result = match codegen.build_app(add_struct.as_val(), &arg_vec[..]).unwrap() {
        Val::Value(v) => {
            let v: IntValue = v.try_into().unwrap();
            assert_eq!(v.get_type().get_bit_width(), 8);
            v
        },
        _ => panic!("Result of building Add should be an int"),
    };
    assert_eq!(app_result.get_zero_extended_constant(), Some(3));
    assert!(app_result.is_const());

    // i.print_to_stderr();
    // let _f_name = f
    //     .get_name()
    //     .to_str()
    //     .expect("Generated name must be valid UTF-8");

    // // Jit
    // let jit_f: JitFunction<unsafe extern "C" fn(b: bool) -> bool> =
    //     unsafe { execution_engine.get_function(_f_name) }.expect("Valid IR generated");

    // // Run
    // unsafe {
    //     assert_eq!(jit_f.call(false), true);
    //     assert_eq!(jit_f.call(true), false);
    // }
}

#[test]
fn bits_mul() {
    let context = Context::create();
    let module = context.create_module("bits");
    // let execution_engine = module
    //     .create_jit_execution_engine(OptimizationLevel::None)
    //     .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let t1 = BitsTy(8).data(3).unwrap();
    // let i1: IntValue = codegen.build(&t1.into())
    // .expect("Compilation works")
    // .try_into()
    // .expect("Compiles values");

    let t2 = BitsTy(8).data(2).unwrap();
    // let i2: IntValue = codegen.build(&t2.into())
    // .expect("Compilation works")
    // .try_into()
    // .expect("Compiles values");

    let add_struct = Mul::new(8).into_var();

    let arg_vec: Vec<ValId> = vec![t1.into(), t2.into()];
    let app_result = match codegen.build_app(add_struct.as_val(), &arg_vec[..]).unwrap() {
        Val::Value(v) => {
            let v: IntValue = v.try_into().unwrap();
            assert_eq!(v.get_type().get_bit_width(), 8);
            v
        },
        _ => panic!("Result of building Add should be an int"),
    };
    assert_eq!(app_result.get_zero_extended_constant(), Some(6));
    assert!(app_result.is_const());

    // i.print_to_stderr();
    // let _f_name = f
    //     .get_name()
    //     .to_str()
    //     .expect("Generated name must be valid UTF-8");

    // // Jit
    // let jit_f: JitFunction<unsafe extern "C" fn(b: bool) -> bool> =
    //     unsafe { execution_engine.get_function(_f_name) }.expect("Valid IR generated");

    // // Run
    // unsafe {
    //     assert_eq!(jit_f.call(false), true);
    //     assert_eq!(jit_f.call(true), false);
    // }
}

#[test]
fn bits_neg() {
    let context = Context::create();
    let module = context.create_module("bits");
    // let execution_engine = module
    //     .create_jit_execution_engine(OptimizationLevel::None)
    //     .unwrap();
    let mut codegen = Codegen::new(&context, module);

    let t1 = BitsTy(8).data(3).unwrap();

    let add_struct = Neg::new(8).into_var();

    let arg_vec: Vec<ValId> = vec![t1.into()];
    let app_result = match codegen.build_app(add_struct.as_val(), &arg_vec[..]).unwrap() {
        Val::Value(v) => {
            let v: IntValue = v.try_into().unwrap();
            assert_eq!(v.get_type().get_bit_width(), 8);
            v
        },
        _ => panic!("Result of building Add should be an int"),
    };
    assert_eq!(app_result.get_zero_extended_constant(), Some(253));
    assert!(app_result.is_const());

    let t1 = BitsTy(8).data(253).unwrap();

    let add_struct = Neg::new(8).into_var();

    let arg_vec: Vec<ValId> = vec![t1.into()];
    let app_result = match codegen.build_app(add_struct.as_val(), &arg_vec[..]).unwrap() {
        Val::Value(v) => {
            let v: IntValue = v.try_into().unwrap();
            assert_eq!(v.get_type().get_bit_width(), 8);
            v
        },
        _ => panic!("Result of building Add should be an int"),
    };
    assert_eq!(app_result.get_zero_extended_constant(), Some(3));
    assert!(app_result.is_const());

}

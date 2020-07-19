use super::*;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;

impl<'ctx> Codegen<'ctx> {
    /// Implement FFI shim
    /// The result function will have a additional argument of type pointer
    /// to the original return type of f if the return type of f is a struct.
    /// In that case, return result will be written to the pointer
    pub fn build_shim(
        &mut self,
        f: FunctionValue<'ctx>,
        name: &str,
        linkage: Option<Linkage>,
    ) -> FunctionValue {
        let f_type = f.get_type();
        let f_args_type = f_type.get_param_types();
        let mut shim_args_type: Vec<BasicTypeEnum<'ctx>> = Vec::new();
        for this_type in f_args_type {
            match this_type {
                BasicTypeEnum::StructType(s) => {
                    // TODO: Address may need to be changed
                    shim_args_type.push(s.ptr_type(AddressSpace::Global).into());
                }
                BasicTypeEnum::IntType(i) => shim_args_type.push(i.into()),
                BasicTypeEnum::PointerType(p) => shim_args_type.push(p.into()),
                _ => unimplemented!(),
            }
        }
        let mut is_return_converted = false;
        let ret_type: BasicTypeEnum<'ctx> = match f_type.get_return_type() {
            Some(t) => {
                match t {
                    BasicTypeEnum::StructType(s) => {
                        // TODO: Address may need to be changed
                        shim_args_type.push(s.ptr_type(AddressSpace::Global).into());
                        is_return_converted = true;
                        self.context.i32_type().into()
                    }
                    BasicTypeEnum::IntType(i) => i.into(),
                    BasicTypeEnum::PointerType(p) => p.into(),
                    _ => unimplemented!(),
                }
            }
            None => unimplemented!("Void return function not implemented"),
        };
        let wrapper_f_type = ret_type.fn_type(&shim_args_type[..], false);
        let wrapper_f = self.module.add_function(name, wrapper_f_type, linkage);
        let this_block = self.context.append_basic_block(wrapper_f, "entry");
        self.builder.position_at_end(this_block);
        let args = if is_return_converted {
            let mut tmp = wrapper_f.get_params();
            tmp.pop();
            tmp
        } else {
            wrapper_f.get_params()
        };
        let mut inner_call_args: Vec<BasicValueEnum<'ctx>> = Vec::new();
        for arg_val in args {
            match arg_val {
                BasicValueEnum::IntValue(i) => inner_call_args.push(i.into()),
                BasicValueEnum::PointerValue(p) => {
                    let this_val = self.builder.build_load(p, "ptr");
                    inner_call_args.push(this_val);
                }
                BasicValueEnum::StructValue(_) => {
                    panic!("Shimed function should not have struct value parameter")
                }
                _ => unimplemented!(),
            }
        }
        match self
            .builder
            .build_call::<FunctionValue<'ctx>>(f, &inner_call_args[..], "call")
            .try_as_basic_value()
            .left()
        {
            Some(v) => {
                if is_return_converted {
                    let _wrapper_params_here = wrapper_f.get_params();
                    match wrapper_f.get_params().pop() {
                        Some(p) => match p {
                            BasicValueEnum::PointerValue(p) => {
                                self.builder.build_store(p, v);
                                self.builder.build_return(Some(
                                    &self.context.i32_type().const_int(0, false),
                                ));
                            }
                            _ => panic!(
                                "Last element of a Shim of a function with 
                        a struct type return should have pointertype"
                            ),
                        },
                        None => panic!(
                            "Shim of a function with 
                    a struct type return should have argument"
                        ),
                    };
                } else {
                    self.builder.build_return(Some(&v));
                }
            }
            None => {
                unimplemented!();
            }
        }
        if let Some(head) = self.head {
            self.builder.position_at_end(head)
        }
        return wrapper_f;
    }
}

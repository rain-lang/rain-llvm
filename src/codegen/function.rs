/*!
Code generations for rain functions
*/
use super::*;
use inkwell::values::FunctionValue;
use rain_ir::function::pi::Pi;

impl<'ctx> Codegen<'ctx> {
    /// Compile a constant `rain` function
    pub fn compile_constant(&mut self, _ty: &Pi, _val: &ValId) -> FunctionValue<'ctx> {
        unimplemented!()
    }
}

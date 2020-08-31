/*!
Code generation for bits type of `rain`
*/

use super::*;
use inkwell::types::IntType;
use rain_ir::primitive::bits::{Bits, BitsTy};
use rain_ir::typing::Typed;
use std::convert::TryFrom;

impl<'ctx> Codegen<'ctx> {
    /// Compile a BitsTy into a LLVM value
    pub fn build_bitsty(&mut self, b: &BitsTy) -> Val<'ctx> {
        unimplemented!("Compile 32-bit LLVM integer constant for BitsTy {}", b)
    }
    /// Get the representation for a bitsTy type
    pub fn repr_bitsty(&mut self, b: &BitsTy) -> Repr<'ctx> {
        let width: u32 = b.0;
        if width == 0 {
            Repr::Empty
        } else if width == 1 {
            Repr::Type(self.context.bool_type().into())
        } else if width <= 8 {
            Repr::Type(self.context.i8_type().into())
        } else if width <= 16 {
            Repr::Type(self.context.i16_type().into())
        } else if width <= 32 {
            Repr::Type(self.context.i32_type().into())
        } else if width <= 64 {
            Repr::Type(self.context.i64_type().into())
        } else if width <= 128 {
            Repr::Type(self.context.i128_type().into())
        } else {
            unimplemented!("Bits type wider than 128 bits has not been implemented.")
        }
    }
    /// compile an bits vector
    pub fn build_bits(&mut self, b: &Bits) -> Val<'ctx> {
        let ty = match b.ty().as_enum() {
            ValueEnum::BitsTy(b) => b,
            _ => unreachable!(),
        };
        let width = ty.0;
        match self.repr_bitsty(ty) {
            Repr::Empty => Val::Contr,
            Repr::Type(t) => {
                let t = IntType::try_from(t).expect("An integer type");
                if b.data() < (1 << width) {
                    t.const_int(b.data() as u64, false).into()
                } else {
                    panic!("The width of Bits {} doesn't match with its type ", b)
                }
            }
            _ => unreachable!(),
        }
    }
}

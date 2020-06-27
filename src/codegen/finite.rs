/*!
Code generation for finite `rain` types
*/
use super::*;
use inkwell::types::IntType;
use rain_ir::primitive::finite::{Finite, Index};
use std::convert::TryFrom;

impl<'ctx> Codegen<'ctx> {
    /// Get the representation for a finite type
    pub fn repr_finite(&mut self, f: &Finite) -> Repr<'ctx> {
        let value: u128 = f.0;
        if value == 0 {
            Repr::Empty
        } else if value == 1 {
            Repr::Prop
        } else if value == 2 {
            Repr::Type(self.context.bool_type().into())
        } else if value < (1 << 8) {
            Repr::Type(self.context.i8_type().into())
        } else if value < (1 << 16) {
            Repr::Type(self.context.i16_type().into())
        } else if value < (1 << 32) {
            Repr::Type(self.context.i32_type().into())
        } else if value < (1 << 64) {
            Repr::Type(self.context.i64_type().into())
        } else {
            Repr::Type(self.context.i128_type().into())
        }
    }
    /// Compile a finite type into an LLVM value
    pub fn build_finite(&mut self, f: &Finite) -> Val<'ctx> {
        unimplemented!(
            "Compile 128-bit LLVM integer constant for finite type {}",
            f
        )
    }
    /// Compile an index into an LLVM value
    pub fn build_index(&mut self, i: &Index) -> Val<'ctx> {
        match self.repr_finite(&i.get_ty()) {
            Repr::Empty => Val::Contr,
            Repr::Prop => Val::Unit,
            Repr::Type(t) => {
                let t = IntType::try_from(t).expect("An integer type");
                if i.ix() <= u64::MAX as u128 {
                    t.const_int(i.ix() as u64, false).into()
                } else {
                    unimplemented!("Big constants...")
                }
            }
            _ => unreachable!(),
        }
    }
}

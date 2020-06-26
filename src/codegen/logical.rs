/*!
Code generation for logical `rain` expressions and types
*/

use super::*;
use either::Either;
use inkwell::values::{FunctionValue, IntValue};
use rain_ir::primitive::logical::{self, Logical, LOGICAL_OP_TYS};
use std::convert::TryInto;

impl<'ctx> Codegen<'ctx> {
    /// Compile a boolean value
    pub fn compile_bool(&mut self, b: bool) -> IntValue<'ctx> {
        self.context.bool_type().const_int(b as u64, false)
    }

    /// Compile a constant logical `rain` function
    pub fn compile_logical(&mut self, l: &Logical) -> FunctionValue<'ctx> {
        if let Some(b) = l.get_const() {
            return self.compile_constant(&LOGICAL_OP_TYS[l.arity() as usize - 1], &b.into());
        }
        match l.arity() {
            1 => match l.data() {
                0b01 => unimplemented!("Logical not compilation"), // logical not
                0b10 => unimplemented!("Logical identity compilation"), // logical identity
                _ => unreachable!(),
            },
            _ => unimplemented!(),
        }
    }

    /// Build the evaluation of a logical operation on an argument list in the current basic block
    pub fn build_logical_expr(&mut self, l: Logical, args: &[ValId]) -> Result<Val<'ctx>, Error> {
        // Arity check
        let l_arity = l.arity() as usize;
        debug_assert!(
            l_arity >= args.len(),
            "Arity (({}).arity() = {}) must be greater or equal to than the length of the argument list ({:?}.len() = {})",
            l, l_arity, args, args.len()
        );
        // Partial logical evaluation check
        if l_arity != args.len() {
            unimplemented!()
        }
        // Direct construction of constant operations
        if let Some(c) = l.get_const() {
            return Ok(self.compile_bool(c).into());
        }
        // Direct construction of non-constant operations
        match l_arity {
            0 => panic!("Zero arity logical operations ({}) are invalid!", l),
            // Unary operations
            1 => {
                let arg = self.build(&args[0])?;
                if l == logical::Not {
                    let arg: IntValue = arg.try_into().expect("A boolean value");
                    return Ok(self.builder.build_not(arg, "pnot").into());
                }
                if l == logical::Id {
                    return Ok(arg);
                }
                panic!("Invalid non-constant unary operation!")
            }
            // Binary operations
            2 => {
                if l == logical::And {
                    let lhs: IntValue = self.build(&args[0])?.try_into().expect("A boolean value");
                    let rhs: IntValue = self.build(&args[1])?.try_into().expect("A boolean value");
                    return Ok(self.builder.build_and(lhs, rhs, "pand").into());
                }
                if l == logical::Or {
                    let lhs: IntValue = self.build(&args[0])?.try_into().expect("A boolean value");
                    let rhs: IntValue = self.build(&args[1])?.try_into().expect("A boolean value");
                    return Ok(self.builder.build_or(lhs, rhs, "por").into());
                }
                if l == logical::Xor {
                    let lhs: IntValue = self.build(&args[0])?.try_into().expect("A boolean value");
                    let rhs: IntValue = self.build(&args[1])?.try_into().expect("A boolean value");
                    return Ok(self.builder.build_xor(lhs, rhs, "pxor").into());
                }
                // Go to general strategy: split and evaluate
            }
            _ => {} // Go to general strategy: split and evaluate
        }
        // General strategy: split and evaluate
        let true_branch = l.apply(true);
        let false_branch = l.apply(false);
        let select = self.build(&args[0])?.try_into().expect("A boolean value");
        let (high, low) = match (true_branch, false_branch) {
            (Either::Left(high), Either::Left(low)) => {
                // Selection between constant booleans: arity 1!
                debug_assert_eq!(l_arity, 1);
                (self.compile_bool(high), self.compile_bool(low))
            }
            (Either::Right(high), Either::Right(low)) => {
                // Selection between function results: arity > 1
                debug_assert!(l_arity > 1);
                let high: IntValue = self
                    .build_logical_expr(high, &args[1..])?
                    .try_into()
                    .expect("A boolean value");
                let low: IntValue = self
                    .build_logical_expr(low, &args[1..])?
                    .try_into()
                    .expect("A boolean value");
                (high, low)
            }
            (t, f) => panic!("Branches {}, {} of {} should have the same arity!", t, f, l),
        };
        let is_high = self.builder.build_and(high, select, "is_high");
        let not_select = self.builder.build_not(select, "nsel");
        let is_low = self.builder.build_and(low, not_select, "is_low");
        Ok(self.builder.build_or(is_high, is_low, "psplit").into())
    }
}

/*!
Code generation for rain gammas
*/
use super::*;
use hayami_im::SymbolStack;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue};
use rain_ir::function::{lambda::Lambda, pi::Pi, gamma::{Gamma, pattern::PatternData}};
use rain_ir::region::{self, Regional};
use rain_ir::typing::Typed;
use rain_ir::value::expr::Sexpr;
use std::ops::Deref;

/// The default linkage of lambda values
pub const DEFAULT_GAMMA_LINKAGE: Option<Linkage> = None;


impl<'ctx> Codegen<'ctx> {

    /// Build a gamma node
    pub fn build_gamma(&mut self, gamma: &Gamma) -> Result<Val<'ctx>, Error> {
        // TODO: Factor this out into a helper function later
        // Step 1: Cache and initialize region
        let old_region = if gamma.depth() != 0 {
            unimplemented!(
                "Closures not implemented for lambda {} (depth = {})!",
                gamma,
                gamma.depth()
            )
        } else {
            self.region.take()
        };

        // Step 2: construct type
        let pi = gamma.get_ty();
        let region = pi.def_region();
        let result = pi.result();
        if result.depth() != 0 {
            return Err(Error::NotImplemented(
                "Non-constant return types for pi functions",
            ));
        }
        let result_repr = match self.repr(result)? {
            Repr::Type(t) => t,
            Repr::Function(_f) => unimplemented!(),
            Repr::Empty | Repr::Prop => return Ok(Val::Unit),
            Repr::Irrep => return Ok(Val::Irrep),
            Repr::Product(p) => p.repr.into(),
        };
        let mut input_reprs: Vec<BasicTypeEnum> = Vec::with_capacity(region.len());
        let mut input_ixes: Vec<isize> = Vec::with_capacity(region.len());
        const PROP_IX: isize = -1;
        const IRREP_IX: isize = -2;
        let mut has_empty = false;

        // Step 2.a: create parameters
        for input_ty in region.data().iter() {
            match self.repr(input_ty)? {
                Repr::Type(t) => {
                    if !has_empty {
                        input_ixes.push(input_reprs.len() as isize);
                        input_reprs.push(t);
                    }
                }
                Repr::Function(_) => unimplemented!(),
                Repr::Prop => {
                    if !has_empty {
                        input_ixes.push(PROP_IX);
                    }
                }
                Repr::Empty => has_empty = true,
                Repr::Irrep => {
                    if !has_empty {
                        input_ixes.push(IRREP_IX);
                    }
                }
                Repr::Product(p) => {
                    if !has_empty {
                        input_ixes.push(input_reprs.len() as isize);
                        input_reprs.push(p.repr.into());
                    }
                }
            }
        }

        // Edge case: function has an empty parameter, so no need to make any code
        if has_empty {
            self.region = old_region; // Reset region!
            return Ok(Val::Unit);
        }

        // Step 3: construct a function type
        let result_ty = result_repr.fn_type(&input_reprs, false);

        // Step 4: construct an empty function of a given type
        let result_fn = self.module.add_function(
            &format!("__lambda_{}", self.counter),
            result_ty,
            DEFAULT_GAMMA_LINKAGE,
        );
        self.counter += 1;

        // Step 5: load parameter vector
        let mut parameter_values: Vec<Val<'ctx>> = Vec::with_capacity(region.len());
        for ix in input_ixes.iter().copied() {
            match ix {
                PROP_IX => {
                    parameter_values.push(Val::Unit);
                }
                IRREP_IX => {
                    parameter_values.push(Val::Irrep);
                }
                ix => {
                    parameter_values.push(Val::Value(
                        result_fn
                            .get_nth_param(ix as u32)
                            .expect("Index in vector is in bounds"),
                    ));
                }
            }
        }

        // Step 6: add an entry basic block, registering it, and setting the builder position
        let entry_bb = self.context.append_basic_block(result_fn, "entry");
        self.builder.position_at_end(entry_bb);

        // Step 7: cache old head, current, and locals, and set new values
        let old_curr = self.curr;
        let old_head = self.head;
        let old_locals = self.locals.take();
        self.curr = Some(result_fn);
        self.head = Some(entry_bb);
        
        if gamma.branches().len() == 1 {
            // Step 8: build the body of this lambda by "inlining it into itself"
            let retv = self.build_lambda_inline(gamma.branches()[0].func(), &parameter_values[..]);

            // Step 9: if successful, build a return instruction
            let retv_build = match retv {
                Ok(retv) => match retv {
                    Val::Value(v) => {
                        self.builder.build_return(Some(&v));
                        Ok(())
                    }
                    Val::Function(f) => unimplemented!(
                        "Higher order functions not yet implemented, returned {:?}",
                        f
                    ),
                    v @ Val::Unit | v @ Val::Irrep | v @ Val::Contr => panic!(
                        "Impossible representation {:?} for compiled function result",
                        v
                    ),
                },
                Err(err) => Err(err),
            };
            // Step 10: Cleanup: reset current, locals, head, and region
            self.curr = old_curr;
            self.head = old_head;
            self.locals = old_locals;
            self.region = old_region;

            // Step 11: Return, handling errors
            // Bubble up retv errors here;
            retv_build?;
            // Otherwise, return successfully constructed function
            Ok(Val::Function(result_fn))
        } else if gamma.branches().len() == 2 {
            let branch_0 = &gamma.branches()[0];
            let this_bool_pattern = match branch_0.pattern().deref() {
                PatternData::Empty(_) => unimplemented!("Fix this later"),
                PatternData::Any(_) => unimplemented!("Fix this later"),
                PatternData::Bool(b) => b
            };
            let this_block = self.context.append_basic_block(result_fn, "branch_0");
            self.builder.position_at_end(this_block);
            let result_0 = self.build_lambda_inline(branch_0.func(), &[])?;
            
            let branch_1 = &gamma.branches()[0];
            

            unimplemented!();
        } else {
            Err(Error::InternalError("Boolean gamma node should not have more than two branches"))
        }
    }

}
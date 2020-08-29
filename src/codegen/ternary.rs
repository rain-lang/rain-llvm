/*!
Code generation for rain gammas
*/
use super::*;
use either::Either;
use inkwell::module::Linkage;
use inkwell::values::{BasicValue, IntValue};
use rain_ir::control::ternary::Ternary;
use rain_ir::region::Regional;
use rain_ir::typing::Typed;

/// The default linkage of lambda values
pub const DEFAULT_GAMMA_LINKAGE: Option<Linkage> = None;

impl<'ctx> Codegen<'ctx> {
    /// Build an inline ternary node, switching on a given LLVM boolean
    ///
    /// # Preconditions
    /// This function assumes that it is called when within an LLVM function, with `switch_bool` a valid `IntValue`
    /// in that function. If this is not the case, `Error::ExpectedFunction` will be returned. Furthermore, it assumes
    /// the function already has an entry basic block (TODO: think about whether this is necessary)
    pub fn build_ternary_inline(
        &mut self,
        ternary: &Ternary,
        switch_bool: IntValue,
    ) -> Result<Val<'ctx>, Error> {
        // Step 0: get the current function and representation, failing early if unavailable
        let curr = self.curr.ok_or(Error::NoCurrentFunction)?;
        let high_ty = ternary.high().ty();
        let low_ty = ternary.low().ty();
        let result_repr = if high_ty == low_ty {
            self.repr(high_ty.as_var())?
        } else {
            unimplemented!(
                "Dependently typed ternary nodes: high_ty = {:?} != low_ty = {:?}, ternary = {:#?}",
                high_ty,
                low_ty,
                ternary
            );
        };
        let result_repr = if let Repr::Type(ty) = result_repr {
            ty
        } else {
            unimplemented!("Non basic representation {:?}", result_repr)
        };

        // Step 1: create branches, build conditional branch
        let high_br = self.context.append_basic_block(curr, "high");
        let low_br = self.context.append_basic_block(curr, "low");
        let result_br = self.context.append_basic_block(curr, "ternary_result");

        self.builder
            .build_conditional_branch(switch_bool, high_br, low_br);

        // Step 2: compile values into high/low branches
        // Step 2.a: high branch
        self.head = Some(high_br);
        self.builder.position_at_end(high_br);
        let high_val = match self.build(&ternary.high())? {
            Val::Value(v) => v,
            v => unimplemented!(
                "Non LLVM branch values not yet implemented: got high branch {:?}",
                v
            ),
        };
        self.builder.build_unconditional_branch(result_br);

        // Step 2.b: low branch
        self.head = Some(low_br);
        self.builder.position_at_end(low_br);
        let low_val = match self.build(&ternary.low())? {
            Val::Value(v) => v,
            v => unimplemented!(
                "Non LLVM branch values not yet implemented: got low branch {:#?}",
                v
            ),
        };
        self.builder.build_unconditional_branch(result_br);

        // Step 3: compile phi result into result branch
        // Note we stay in the result branch at the end, since further instructions should be placed there
        self.head = Some(result_br);
        self.builder.position_at_end(result_br);
        let phi_val = self.builder.build_phi(result_repr, "tern");
        phi_val.add_incoming(&[(&high_val, high_br), (&low_val, low_br)]);

        // Step 4: return
        Ok(Val::Value(phi_val.as_basic_value()))
    }

    /// Build a ternary node
    pub fn build_ternary(&mut self, ternary: &Ternary) -> Result<Val<'ctx>, Error> {
        // TODO: Factor this out into a helper function later
        if ternary.low().ty() != ternary.high().ty() {
            unimplemented!("Dependent ternary nodes are not implemented");
        }
        // Step 1: Cache and initialize region
        let old_region = if ternary.depth() != 0 {
            unimplemented!(
                "Closures not implemented for ternary nodes {} (depth = {})!",
                ternary,
                ternary.depth()
            )
        } else {
            self.region.clone()
        };
        self.region = ternary.region().clone_region();

        // Step 2: construct prototype, construct function, handle edge cases
        //TODO: general get_repr
        let prototype_or_return = match self.build_function_repr(ternary.get_ty()) {
            Ok(Repr::Function(prototype)) => Either::Left(prototype),
            Ok(Repr::Prop) => Either::Right(Ok(Val::Unit)),
            Ok(r) => panic!("Invalid function representation: {:?}", r),
            Err(err) => Either::Right(Err(err)),
        };

        let prototype = match prototype_or_return {
            Either::Left(prototype) => prototype,
            Either::Right(retv) => {
                // Reset region
                self.region = old_region;
                // Propagate early return, avoiding codegen
                return retv;
            }
        };

        let result_fn = self.module.add_function(
            &format!("__tern_{}", self.counter),
            prototype.repr,
            DEFAULT_GAMMA_LINKAGE,
        );
        self.counter += 1;

        // Step 3: add an entry basic block, registering it, and setting the builder position
        let entry_bb = self.context.append_basic_block(result_fn, "entry");
        self.builder.position_at_end(entry_bb);

        // Step 4: cache old head, current, and locals, and set new values
        let old_curr = self.curr;
        let old_head = self.head;
        let old_locals = self.locals.take();
        self.curr = Some(result_fn);
        self.head = Some(entry_bb);

        // Step 5: compile ternary, caching error
        let boolean_param = result_fn.get_nth_param(0).unwrap().into_int_value();
        let ternary_result = self.build_ternary_inline(ternary, boolean_param);

        // Step 6: build return
        if let Ok(ternary_result) = &ternary_result {
            let return_value: Option<&dyn BasicValue> = match ternary_result {
                Val::Value(v) => Some(&*v),
                Val::Function(f) => unimplemented!("Function return for {:?}", f),
                _ => None,
            };
            self.builder.build_return(return_value);
        };

        // Step 7: Cleanup: reset current, locals, head, and region, and propagate errors if necessary
        // Debug assertions: note that `head` and `locals` are allowed to change
        debug_assert_eq!(self.region.region(), ternary.region());
        debug_assert_eq!(self.curr, Some(result_fn));

        // Resets:
        self.curr = old_curr;
        self.head = old_head;
        if let Some(head) = old_head {
            self.builder.position_at_end(head);
        }
        self.locals = old_locals;
        self.region = old_region;

        ternary_result?;

        // Otherwise, return successfully constructed function
        Ok(Val::Function(result_fn))
    }
}

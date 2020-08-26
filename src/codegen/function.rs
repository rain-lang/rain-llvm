/*!
Code generation for rain functions
*/
use super::*;
use either::Either;
use hayami_im_rc::SymbolStack;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue};
use rain_ir::function::{lambda::Lambda, pi::Pi};
use rain_ir::region::Regional;
use rain_ir::typing::Typed;
use rain_ir::value::expr::Sexpr;
use std::rc::Rc;
use std::convert::TryInto;

/// The default linkage of lambda values
pub const DEFAULT_LAMBDA_LINKAGE: Option<Linkage> = None;

impl<'ctx> Codegen<'ctx> {
    /// Build a constant `rain` function
    pub fn build_constant(&mut self, _ty: &Pi, _val: &ValId) -> FunctionValue<'ctx> {
        unimplemented!()
    }

    /// Build a function call with arguments
    pub fn build_function_call(
        &mut self,
        f: FunctionValue<'ctx>,
        args: &[ValId],
    ) -> Result<Val<'ctx>, Error> {
        let mut this_args: Vec<BasicValueEnum<'ctx>> = Vec::new();
        for arg in args {
            match self.build(arg)? {
                Val::Contr => return Ok(Val::Contr),
                Val::Unit => {
                    return Ok(Val::Unit);
                }
                Val::Value(v) => this_args.push(v),
                Val::Function(_) => unimplemented!("Higher order functions not yet implemented!"),
            }
        }
        match self
            .builder
            .build_call::<FunctionValue<'ctx>>(f, &this_args[..], "call")
            .try_as_basic_value()
            .left()
        {
            Some(b) => Ok(b.into()),
            None => Ok(Val::Unit),
        }
    }

    /// Build a function application
    pub fn build_app(&mut self, f: &ValId, args: &[ValId]) -> Result<Val<'ctx>, Error> {
        if args.is_empty() {
            return self.build(f);
        }

        match f.as_enum() {
            ValueEnum::Logical(l) => return self.build_logical_expr(*l, args),
            ValueEnum::Add(_a) => {
                if args.len() != 2 {
                    unimplemented!();
                }
                let arg_0 = match args[0].as_enum() {
                    ValueEnum::Bits(b) => self.build_bits(b),
                    _ => unimplemented!(),
                };
                let arg_1 = match args[1].as_enum() {
                    ValueEnum::Bits(b) => self.build_bits(b),
                    _ => unimplemented!(),
                };
                match (arg_0, arg_1) {
                    (Val::Value(v1), Val::Value(v2)) => {
                        let int_1: IntValue<'ctx> = v1.try_into().unwrap();
                        let int_2: IntValue<'ctx> = v2.try_into().unwrap();
                        let result = self.builder.build_int_add(int_1, int_2, "__add_");
                        return Ok(Val::Value(result.into()))
                    },
                    _ => unimplemented!("Add only applies to bits"),
                }
            },
            _ => {}
        }

        let ty = f.ty();

        match ty.as_enum() {
            ValueEnum::Product(_p) => {
                match self.repr(&ty.clone_ty())? {
                    Repr::Prop => Ok(Val::Unit),
                    Repr::Empty => Ok(Val::Contr),
                    Repr::Type(_t) => unimplemented!(),
                    Repr::Function(_f) => unimplemented!(),
                    Repr::Product(p) => {
                        // Generate GEP.
                        if args.len() != 1 {
                            unimplemented!();
                        }
                        let ix = match args[0].as_enum() {
                            ValueEnum::Index(ix) => ix.ix() as usize,
                            _ => unimplemented!(),
                        };
                        let repr_ix = if let Some(ix) = p.mapping.get(ix) {
                            ix
                        } else {
                            return Ok(Val::Unit);
                        };
                        let struct_value = match self.build(f)? {
                            Val::Value(BasicValueEnum::StructValue(s)) => s,
                            Val::Contr => return Ok(Val::Contr),
                            _ => panic!("Internal error: Repr::Product guarantees BasicValueEnum::StructValue")
                        };
                        let element = self
                            .builder
                            .build_extract_value(struct_value, repr_ix, "idx")
                            .expect("Internal error: valid index guaranteed by IR construction");
                        Ok(Val::Value(element))
                    }
                }
            }
            ValueEnum::Lambda(l) => match self.build_lambda(l)? {
                Val::Contr => Ok(Val::Contr),
                Val::Unit => unimplemented!("Unit lambda representation"), //TODO: think about this...
                Val::Value(v) => unimplemented!("Value lambda representation {:?}", v),
                Val::Function(f) => self.build_function_call(f, args),
            },
            v => unimplemented!("Application of value {}", v),
        }
    }

    /// Build an S-expression
    pub fn build_sexpr(&mut self, s: &Sexpr) -> Result<Val<'ctx>, Error> {
        if s.len() == 0 {
            return Ok(Val::Unit);
        }
        self.build_app(&s[0], &s.as_slice()[1..])
    }

    /// Build an inline lambda function with given parameter_values
    pub fn build_lambda_inline(
        &mut self,
        lambda: &Lambda,
        parameter_values: &[Val<'ctx>],
    ) -> Result<Val<'ctx>, Error> {
        // Step 1: get Least Common Region between this lambda's region and the current region
        let gcr = self.curr_region.gcr(lambda)?;

        // Step 2: cache the old symbol table, and push a new one
        let mut base = self.locals.as_ref();
        let dd = self.curr_region.depth() - gcr.depth();
        for _ in 0..dd {
            base = base
                .expect("Too few layers in symbol table for region")
                .prev();
        }
        let new_table = if let Some(base) = base {
            //TODO: use `Rc`-reference for efficiency here, special casing dd-0
            base.clone().extend()
        } else {
            SymbolTable::default()
        };
        let old_table = self.locals.replace(new_table);

        // Step 3: register parameters
        let locals = self.locals.as_mut().unwrap();
        for (i, val) in parameter_values.iter().enumerate() {
            let valid = ValId::from(
                lambda
                    .get_ty()
                    .def_region()
                    .clone()
                    .param(i)
                    .expect("Iterated index is in bounds"),
            );
            locals.insert(valid, val.clone());
        }

        // Step 4: compute result
        let result = self.build(lambda.result());

        // Step 5: restore the old table
        self.locals = old_table;

        // Step 6: return
        result
    }

    /// Build a function representation
    pub fn build_function_repr(&mut self, pi: &Pi) -> Result<Repr<'ctx>, Error> {
        // Step 1: Compute result representation
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
            Repr::Prop | Repr::Empty => return Ok(Repr::Prop),
            Repr::Product(p) => p.repr.into(),
        };

        // Step 2: Compute parameter types
        let mut input_reprs: Vec<BasicTypeEnum> = Vec::with_capacity(region.len());
        let mut input_ixes: IxMap = IxMap::with_capacity(region.len() as u32);
        let mut has_empty = false;

        for input_ty in region.param_tys().iter() {
            match self.repr(input_ty)? {
                Repr::Type(t) => {
                    if !has_empty {
                        input_ixes.push_ix(input_reprs.len() as u32);
                        input_reprs.push(t);
                    }
                }
                Repr::Function(_) => unimplemented!(),
                Repr::Prop => {
                    if !has_empty {
                        input_ixes.push_prop();
                    }
                }
                Repr::Empty => has_empty = true,
                Repr::Product(p) => {
                    if !has_empty {
                        input_ixes.push_ix(input_reprs.len() as u32);
                        input_reprs.push(p.repr.into());
                    }
                }
            }
        }

        // Edge case: function has an empty parameter, so no need to make any code
        if has_empty {
            return Ok(Repr::Prop);
        }

        // Step 3: create LLVM function type
        let repr = result_repr.fn_type(&input_reprs, false);

        Ok(Repr::Function(Rc::new(FunctionRepr {
            repr,
            mapping: input_ixes,
        })))
    }

    /// Build a `rain` lambda function
    pub fn build_lambda(&mut self, lambda: &Lambda) -> Result<Val<'ctx>, Error> {
        // Step 1: Cache and initialize region
        let old_region = if lambda.depth() != 0 {
            unimplemented!(
                "Closures not implemented for lambda {} (depth = {})!",
                lambda,
                lambda.depth()
            )
        } else {
            self.region.clone()
        };

        // Step 2: construct prototype, construct function, handle edge cases
        //TODO: general get_repr
        let prototype_or_return = match self.build_function_repr(lambda.get_ty()) {
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
            &format!("__lambda_{}", self.counter),
            prototype.repr,
            DEFAULT_LAMBDA_LINKAGE,
        );
        self.counter += 1;

        // Step 3: set region, load parameter vector
        let region = lambda.def_region();
        self.region = region.clone_region();

        let mut parameter_values: Vec<Val<'ctx>> = Vec::with_capacity(region.len());
        for ix in prototype.mapping.iter() {
            match ix {
                ReprIx::Prop => {
                    parameter_values.push(Val::Unit);
                }
                ReprIx::Val(ix) => {
                    parameter_values.push(Val::Value(
                        result_fn
                            .get_nth_param(ix as u32)
                            .expect("Index in vector is in bounds"),
                    ));
                }
            }
        }

        // Step 4: add an entry basic block, registering it, and setting the builder position
        let entry_bb = self.context.append_basic_block(result_fn, "entry");
        self.builder.position_at_end(entry_bb);

        // Step 5: cache old head, current, and locals, and set new values
        let old_curr = self.curr;
        let old_head = self.head;
        let old_locals = self.locals.take();
        self.curr = Some(result_fn);
        self.head = Some(entry_bb);

        // Step 6: build the body of this lambda by "inlining it into itself"
        let retv = self.build_lambda_inline(lambda, &parameter_values[..]);

        // Step 7: if successful, build a return instruction
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
                v @ Val::Unit | v @ Val::Contr => panic!(
                    "Impossible representation {:?} for compiled function result",
                    v
                ),
            },
            Err(err) => Err(err),
        };

        // Step 8: Cleanup: reset current, locals, head, and region

        // Debug assertions: note that `head` and `locals` are allowed to change
        debug_assert_eq!(&self.region, region);
        debug_assert_eq!(self.curr, Some(result_fn));

        // Resets
        self.curr = old_curr;
        self.head = old_head;
        if let Some(head) = old_head {
            self.builder.position_at_end(head);
        }
        self.locals = old_locals;
        self.region = old_region;

        // Step 9: Return, handling errors
        // Bubble up retv errors here;
        retv_build?;
        // Otherwise, return successfully constructed function
        Ok(Val::Function(result_fn))
    }
}

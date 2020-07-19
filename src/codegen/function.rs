/*!
Code generation for rain functions
*/
use super::*;
use hayami_im::SymbolStack;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue};
use rain_ir::function::{lambda::Lambda, pi::Pi};
use rain_ir::region::{self, Regional};
use rain_ir::typing::Typed;
use rain_ir::value::expr::Sexpr;

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
                Val::Irrep => return Err(Error::Irrepresentable), //TODO: return Ok(Val::Irrep)?
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

        //TODO: more generic inline?
        if let ValueEnum::Logical(l) = f.as_enum() {
            return self.build_logical_expr(*l, args)
        }

        let ty = f.ty();

        match ty.as_enum() {
            ValueEnum::Product(_p) => {
                match self.repr(&ty.clone_ty())? {
                    Repr::Prop => Ok(Val::Unit),
                    Repr::Empty => Ok(Val::Contr),
                    Repr::Irrep => Ok(Val::Irrep),
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
                        let repr_ix = if let Some(ix) = p.mapping[ix] {
                            ix
                        } else {
                            return Ok(Val::Unit);
                        };
                        let struct_value = match self.build(f)? {
                            Val::Value(BasicValueEnum::StructValue(s)) => s,
                            Val::Contr => return Ok(Val::Contr),
                            Val::Irrep => return Ok(Val::Irrep), //TODO: think about this...
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
                Val::Irrep => Ok(Val::Irrep), //TODO: think about this...
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
        let lcr = region::lcr(lambda, &self.curr_region);

        // Step 2: cache the old symbol table, and push a new one
        let mut base = self.locals.as_ref();
        let dd = self.curr_region.depth() - lcr.depth();
        for _ in 0..dd {
            base = base.expect("Too few layers in symbol table for region").prev();
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
            self.region.take()
        };

        // Step 2: construct type
        let pi = lambda.get_ty();
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
            DEFAULT_LAMBDA_LINKAGE,
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
        // Step 8: build the body of this lambda by "inlining it into itself"
        let retv = self.build_lambda_inline(lambda, &parameter_values[..]);

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
    }
}

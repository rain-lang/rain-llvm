/*!
Code generation for rain functions
*/
use super::*;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue};
use rain_ir::function::{lambda::Lambda, pi::Pi};
use rain_ir::region::Regional;
use rain_ir::typing::Typed;
use rain_ir::value::expr::Sexpr;

/// A prototype for a `rain` function
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Prototype<'ctx> {
    /// An LLVM function value
    Function(FunctionValue<'ctx>),
    /// A unit function, i.e. a function whose return type is a unit type or contradiction
    /// *or* which has a contradiction as an argument
    Unit,
    /// An irrepresentable function, which has irrepresentable arguments and a non unit/contradiction
    /// return type *and* no contradiction arguments
    Irrep,
}

/// The default linkage of lambda values
pub const DEFAULT_LAMBDA_LINKAGE: Option<Linkage> = None;

impl<'ctx> Codegen<'ctx> {
    /// Build a constant `rain` function
    pub fn build_constant(&mut self, _ty: &Pi, _val: &ValId) -> FunctionValue<'ctx> {
        unimplemented!()
    }

    /// Create a function prototype for a lambda function, binding its parameters
    pub fn build_prototype(&mut self, lambda: &Lambda) -> Result<Prototype<'ctx>, Error> {
        if lambda.depth() != 0 {
            unimplemented!(
                "Closures not implemented for lambda {} (depth = {})!",
                lambda,
                lambda.depth()
            )
        }
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
            Repr::Empty | Repr::Prop => return Ok(Prototype::Unit),
            Repr::Irrep => return Ok(Prototype::Irrep),
            Repr::Product(p) => p.repr.into(),
        };
        let mut input_reprs: Vec<BasicTypeEnum> = Vec::with_capacity(region.len());
        let mut input_ixes: Vec<isize> = Vec::with_capacity(region.len());
        const PROP_IX: isize = -1;
        const IRREP_IX: isize = -2;
        let mut has_empty = false;

        for input_ty in region.iter() {
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

        if has_empty {
            return Ok(Prototype::Unit);
        }

        // Construct a function type
        let result_ty = result_repr.fn_type(&input_reprs, false);

        // Construct an empty function of a given type
        let result_fn = self.module.add_function(
            &format!("__lambda_{}", self.counter),
            result_ty,
            DEFAULT_LAMBDA_LINKAGE,
        );
        self.counter += 1;

        self.curr_ix = self.local_arena.push(SymbolTable::default());
        self.local_ixs.insert(result_fn, (self.curr_ix, None));

        let this_table = match self.local_arena.get_mut(self.curr_ix) {
            Some(t) => t,
            None => panic!("A symbol table should be pushed when building a prototype"),
        };
        // Bind parameters
        for (i, ix) in input_ixes.iter().copied().enumerate() {
            let valid = ValId::from(
                region
                    .clone()
                    .param(i)
                    .expect("Iterated index is in bounds"),
            );
            match ix {
                PROP_IX => {
                    this_table.insert(valid, Val::Unit);
                }
                IRREP_IX => {
                    this_table.insert(valid, Val::Irrep);
                }
                ix => {
                    this_table.insert(
                        valid,
                        Val::Value(
                            result_fn
                                .get_nth_param(ix as u32)
                                .expect("Index in vector is in bounds")
                                .into(),
                        ),
                    );
                }
            }
        }

        Ok(Prototype::Function(result_fn))
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
        if args.len() == 0 {
            return self.build(f);
        }
        match f.as_enum() {
            // Special case logical operation building
            ValueEnum::Logical(l) => return self.build_logical_expr(*l, args),
            _ => {}
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
                        Ok(Val::Value(element.into()))
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

    /// Build a `rain` lambda function
    pub fn build_lambda(&mut self, lambda: &Lambda) -> Result<Val<'ctx>, Error> {
        // Caching the old one, new one will be set in build_prototype
        let old_curr = self.curr;
        let old_curr_ix = self.curr_ix;
        // Get the function to build
        let f = match self.build_prototype(lambda)? {
            Prototype::Function(f) => f,
            Prototype::Unit => return Ok(Val::Unit),
            Prototype::Irrep => return Ok(Val::Irrep),
        };
        // Add an entry basic block, registering it
        let entry_bb = self.context.append_basic_block(f, "entry");
        self.local_ixs.insert(
            f, 
            (self.curr_ix, Some(entry_bb))
        );
        self.builder.position_at_end(entry_bb);
        // Build a return value for the current function
        let retv = self.build(lambda.result());
        // If successful, build a return instruction
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
        // Either way, reset the build head if necessary
        self.curr = old_curr;
        self.curr_ix = old_curr_ix;
        if let Some(curr) = self.curr {
            if let Some((_, b)) = self.local_ixs.get(&curr) {
                if let Some(block) = b {
                    self.builder.position_at_end(*block);
                }
            }
        }
        // Bubble up retv errors here;
        retv_build?;
        // Remove function from table and free the index in arena.
        if let Some((i, _)) = self.local_ixs.remove(&f) {
            self.local_arena.free(i);
        }

        // Otherwise, return successfully constructed function
        Ok(Val::Function(f))
    }
}

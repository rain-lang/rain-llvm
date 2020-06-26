/*!
Code generation for rain functions
*/
use super::*;
use inkwell::values::{BasicValueEnum, FunctionValue};
use rain_ir::function::{lambda::Lambda, pi::Pi};
use rain_ir::typing::Typed;
use rain_ir::value::expr::Sexpr;

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
                Val::Function(f) => self.build_function_call(f, args)
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
    pub fn build_lambda(&mut self, _l: &Lambda) -> Result<Val<'ctx>, Error> {
        unimplemented!("Lambda construction")
    }
}

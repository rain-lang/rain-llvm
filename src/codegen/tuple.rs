/*!
Code generation for `rain` tuples and product types
*/

use super::*;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use rain_ir::typing::Typed;
use rain_ir::value::tuple::{Product, Tuple};
use std::rc::Rc;

impl<'ctx> Codegen<'ctx> {
    /// Get the representation for a product type
    pub fn repr_product(&mut self, p: &Product) -> Result<Repr<'ctx>, Error> {
        let mut mapping: Vec<Option<u32>> = Vec::new();
        let mut struct_index = 0;
        let mut repr_vec: Vec<BasicTypeEnum<'ctx>> = Vec::new();
        let mut reprs = p.iter().map(|ty| self.repr(ty));
        while let Some(repr) = reprs.next() {
            let repr = repr?;
            match repr {
                Repr::Type(ty) => {
                    repr_vec.push(ty);
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
                Repr::Function(f) => {
                    repr_vec.push(f.ptr_type(AddressSpace::Global).into());
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
                Repr::Empty => return Ok(Repr::Empty),
                Repr::Irrep => {
                    let mut return_empty = false;
                    for r in reprs {
                        if r? == Repr::Empty {
                            return_empty = true;
                        }
                    }
                    if return_empty {
                        return Ok(Repr::Empty);
                    } else {
                        break;
                    }
                }
                Repr::Prop => mapping.push(None),
                Repr::Product(p) => {
                    repr_vec.push(p.repr.into());
                    mapping.push(Some(struct_index));
                    struct_index += 1;
                }
            }
        }
        if struct_index == 0 {
            Ok(Repr::Empty)
        } else {
            let repr = self.context.struct_type(&repr_vec[..], false);
            Ok(Repr::Product(Rc::new(ProductRepr { mapping, repr })))
        }
    }

    /// Build a product in the current local context
    pub fn build_product(&mut self, _p: &Product) -> Result<Val<'ctx>, Error> {
        unimplemented!("Product type compilation")
    }

    /// Build a tuple in the current local context
    pub fn build_tuple(&mut self, t: &Tuple) -> Result<Val<'ctx>, Error> {
        let p_enum = t.ty().as_enum();
        match p_enum {
            ValueEnum::Product(product) => {
                let repr = match self.repr_product(product)? {
                    Repr::Product(tmp) => tmp,
                    Repr::Prop => return Ok(Val::Unit),
                    Repr::Empty => return Ok(Val::Contr),
                    // TODO: think about Local::Irrep
                    Repr::Irrep => return Err(Error::Irrepresentable),
                    // TODO: Rethink the following later
                    Repr::Function(_f) => {
                        return Err(Error::NotImplemented("Function in tuple not implemented"));
                    }
                    Repr::Type(_t) => {
                        return Err(Error::NotImplemented("Type in tuple not supported yet"))
                    }
                };
                let mut values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for (i, mapped) in repr.mapping.iter().enumerate() {
                    if let Some(_mapped_pos) = mapped {
                        let this_result = self.build(&t[i])?;
                        // Note: This assumes that each type has unique representation
                        let value: BasicValueEnum<'ctx> = match this_result {
                            Val::Value(v) => v,
                            Val::Function(_) => unimplemented!("Function tuple members"),
                            l => panic!("Invalid tuple member {:?}", l),
                        };
                        values.push(value);
                    }
                }
                Ok(Val::Value(repr.repr.const_named_struct(&values[..]).into()))
            }
            ty => panic!(
                "Expected tuple {} to have a product type, but type {} returned instead",
                t, ty
            ),
        }
    }
}

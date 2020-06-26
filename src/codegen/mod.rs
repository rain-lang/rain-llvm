/*!
The data-structures necessary for `rain` code generation
*/
use super::repr::*;
use crate::error::Error;
use fxhash::FxHashMap as HashMap;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use rain_ir::value::{NormalValue, TypeId, ValId, ValueEnum};
use std::ptr::NonNull;

mod finite;
mod function;
mod logical;
mod tuple;

/**
A `rain` code generation context for a given module.

Code generation for most `rain` values should be implemented as methods modifying this struct, which can potentially then
be accessed asynchronously by a single-threaded executor for an `async` compilation model. Thought should be given to
potential generalizations to a multi-threaded compilation model, with separate LLVM modules being generated by each thread
(think in terms of `rustc`s codegen units), but that should come later.
*/
#[derive(Debug)]
pub struct Codegen<'ctx> {
    /// Compiled values
    ///
    /// ## Implementation notes
    /// `(context, value)` pairs are mapped to LLVM representations by this member. `context` is stored as a
    /// `*const NormalValue` to avoid unnecessary atomic operations: the injection from `*const NormalValue`s stored
    /// in this map and contexts is preserved by the fact that, as long as entries for a context are being into this
    /// map, the context is either being kept alive by some object somewhere *or* has been inserted into this hashmap
    /// Here, the `NULL` pointer corresponds to a global, i.e. constant, `rain` value.
    ///
    /// ## Ideas
    /// One future implementation direction could be to add a `HashMap` from the `ValId` for a context to a vector of the
    /// members it holds. This could ensure the above property more cleanly, avoiding bugs, and allow for the possibility
    /// of easily garbage collecting all `vals` members pertinent to a certain `context` without scanning the entire map.
    /// On the other hand, this may be duplicating functionality from, e.g., the `deps` member of `Lambda`, `Pi`, etc., and
    /// or may not be worth the performance and memory costs.
    vals: HashMap<(*const NormalValue, ValId), Val<'ctx>>,
    /// A hashmap of contexts to the current basic block for each context.
    ///
    /// ## Implementation Notes
    /// `NonNull` is used since the global context has no basic block, having no run-time control flow, and hence the
    /// `NULL` context should never have an entry in this hash map.
    ///
    /// ## Ideas
    /// Perhaps reconsider for runtime initialization, e.g. environment variables.
    heads: HashMap<NonNull<NormalValue>, BasicBlock<'ctx>>,
    /// Type representations
    ///
    /// ## Implementation Notes
    /// `rain` types are mapped to their, currently unique, LLVM representations. A more complex solution will probably have to
    /// be sought for closures, dependent types, and unions, but this will have to do for now.
    ///
    /// ## Ideas
    /// This map may be partially or completely replaced with a per-value representation mapping. Alternatively, non-constant types
    /// could be mapped to unions here, though this would require mapping to the same union across different functions using the same
    /// region. This could be avoided by tagging mappings with a `*const NormalValue` context as in `vals`, though is probably
    /// less important (and may even evolve into an important ABI property for at least some calling conventions).
    reprs: HashMap<TypeId, Repr<'ctx>>,
    /// Function name counter.
    ///
    /// ## Ideas
    /// Consider using a hash function to generate function names, since the order of function naming should be unspecified anyways.
    /// Consider a bijective map of values to function names, to avoid collisions. Consider whether hashed names should be inserted
    /// into the map: potentially insert them in only one direction of the map (i.e. `name -> function`, since the name can already be
    /// derived from the function). Would be interesting to have only collisions and manual namings register in the other direction.
    /// FFI will also be an issue, as will shims...
    counter: usize,
    /// The LLVM module to which these values are being added
    module: Module<'ctx>,
    /// The IR builder for this codegen context
    builder: Builder<'ctx>,
    /// The enclosing context of this codegen context
    context: &'ctx Context,
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new, empty code-generation context bound to a given LLVM `context` and `module`
    pub fn new(context: &'ctx Context, module: Module<'ctx>) -> Codegen<'ctx> {
        Codegen {
            vals: HashMap::default(),
            heads: HashMap::default(),
            reprs: HashMap::default(),
            counter: 0,
            module,
            builder: context.create_builder(),
            context,
        }
    }
    /// Get the compiled values in this context
    ///
    /// See the documentation for the `vals` private member of `Codegen` for more information.
    #[inline]
    pub fn vals(&self) -> &HashMap<(*const NormalValue, ValId), Val<'ctx>> {
        &self.vals
    }
    /// Get the compiled representations in this context
    ///
    /// See the documentation for the `reprs` private member of `Codegen` for more information.
    #[inline]
    pub fn reprs(&self) -> &HashMap<TypeId, Repr<'ctx>> {
        &self.reprs
    }
    /// Get the representation for a given type, if any
    pub fn repr(&mut self, t: &TypeId) -> Result<Repr<'ctx>, Error> {
        // Special cases
        match t.as_enum() {
            ValueEnum::BoolTy(_) => return Ok(Repr::Type(self.context.bool_type().into())),
            _ => {}
        }
        // Cached case
        if let Some(repr) = self.reprs.get(t) {
            return Ok(repr.clone());
        }
        // General case
        let r = match t.as_enum() {
            ValueEnum::Finite(f) => self.repr_finite(f),
            ValueEnum::BoolTy(_) => unreachable!(),
            _ => unimplemented!("Representation for rain type {} is not implemented", t)
        };
        let old = self.reprs.insert(t.clone(), r.clone());
        // We just checked above that the type has no representation!
        // TODO: think about this: perhaps compiling the type has led to it getting a representation...
        // consider an in-progress compilation map to avoid infinite loops...
        // No wait, this should be prevented by the DAG-property of the `rain` graph...
        debug_assert_eq!(old, None);
        Ok(r)
    }
}

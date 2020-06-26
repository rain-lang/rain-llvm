/*!
The data-structures necessary for `rain` code generation
*/
use fxhash::FxHashMap as HashMap;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use rain_ir::value::{NormalValue, TypeId, ValId};
use std::ptr::NonNull;

use super::repr::*;

/**
A `rain` code generation context for a given module
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
    /// `rain` types are mapped to their, currently unique, LLVM representations. A more complex solution will probably have to
    /// be sought for, e.g., closures, but this will have to do for now.
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

/*!
`rain`-to-LLVM code generation
*/
#![forbid(missing_docs, missing_debug_implementations)]
#[warn(clippy::all)]

pub mod codegen;
pub mod error;
pub mod repr;

pub use inkwell::context::Context;
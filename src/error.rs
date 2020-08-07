/*!
Error handling
*/
use rain_ir::value;

/// A `rain` code generation error
#[derive(Debug, Clone)]
pub enum Error {
    /// Attempted to create a non-constant value as a constant
    NotConst,
    /// Attempted to create a non-constant value of an irrepresentable type
    Irrepresentable,
    /// Invalid function representation
    InvalidFuncRepr,
    /// No current function set
    NoCurrentFunction,
    /// No curent basic block set
    NoCurrentBlock,
    /// An internal error
    InternalError(&'static str),
    /// Not implemented
    NotImplemented(&'static str),
    /// A `rain` value error
    ValueError(value::Error),
}

impl From<value::Error> for Error {
    fn from(error: value::Error) -> Error {
        Error::ValueError(error)
    }
}

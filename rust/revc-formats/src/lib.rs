//! Loaders for the file formats in a Vice City install.
//!
//! Every parser works on a byte slice and returns owned data or a [`Error`]; nothing here touches the
//! filesystem except [`img::Archive::open`]. The parsers follow what reVC's C++ loaders accept, so
//! a file the game can load must parse here (checked by the `install` test over a real game copy).

pub mod bytes;
pub mod col;
pub mod dff;
pub mod gxt;
pub mod img;
pub mod rw;
pub mod scm;
pub mod text;
pub mod txd;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Input ended before a field could be read: (what, offset).
    Truncated(&'static str, usize),
    /// A value is not what the format allows.
    Invalid(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated(what, at) => write!(f, "truncated {what} at offset {at}"),
            Error::Invalid(msg) => write!(f, "invalid data: {msg}"),
            Error::Io(msg) => write!(f, "i/o error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

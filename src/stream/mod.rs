//! Compress and decompress Zstd streams.
//!
//! This module provide a `Read`/`Write` interface
//! to zstd streams of arbitrary length.
//!
//! They are compatible with the `zstd` command-line tool.

pub mod read;
pub mod write;

mod functions;
pub mod zio;

#[cfg(test)]
mod tests;

pub mod raw;

pub use self::functions::{copy_decode, copy_encode, decode_all, encode_all};
pub use self::read::Decoder;
pub use self::write::{AutoFinishEncoder, Encoder};

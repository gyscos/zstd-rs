//! Compress and decompress Zstd streams.
//!
//! This module provide a `Read`/`Write` interface
//! to zstd streams of arbitrary length.
//!
//! They are compatible with the `zstd` command-line tool.

mod decoder;
mod encoder;

mod functions;
mod zio;

#[cfg(test)]
mod tests;

pub mod raw;

pub use self::decoder::Decoder;
pub use self::encoder::{AutoFinishEncoder, Encoder};
pub use self::functions::{copy_decode, copy_encode, decode_all, encode_all};

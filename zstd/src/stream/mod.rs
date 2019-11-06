//! Compress and decompress Zstd streams.
//!
//! Zstd streams are the main way to compress and decompress data.
//! They are compatible with the `zstd` command-line tool.
//!
//! This module provides both `Read` and `Write` interfaces to compressing and
//! decompressing.

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

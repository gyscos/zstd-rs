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

#[doc(hidden)]
#[macro_export]
macro_rules! readwritecommon {
    ($readwrite:ident) => {
        /// Controls whether zstd should include a content checksum at the end
        /// of each frame.
        pub fn include_checksum(
            &mut self,
            include_checksum: bool,
        ) -> io::Result<()> {
            self.$readwrite.operation_mut().set_parameter(
                zstd_safe::CParameter::ChecksumFlag(include_checksum),
            )
        }

        /// Enables multithreaded compression
        ///
        /// * If `n_workers == 0` (default), then multithreaded will be
        ///   disabled.
        /// * If `n_workers >= 1`, then compression will be done in separate
        ///   threads.
        ///
        /// So even `n_workers = 1` may increase performance by separating
        /// IO and compression.
        pub fn multithread(&mut self, n_workers: u32) -> io::Result<()> {
            self.$readwrite
                .operation_mut()
                .set_parameter(zstd_safe::CParameter::NbWorkers(n_workers))
        }

        /// Enables or disables storing of the dict id.
        ///
        /// Defaults to true. If false, the behaviour of decoding with a wrong
        /// dictionary is undefined.
        pub fn include_dictid(
            &mut self,
            include_dictid: bool,
        ) -> io::Result<()> {
            self.$readwrite.operation_mut().set_parameter(
                zstd_safe::CParameter::DictIdFlag(include_dictid),
            )
        }

        /// Enables or disabled storing of the contentsize
        pub fn include_contentsize(
            &mut self,
            include_contentsize: bool,
        ) -> io::Result<()> {
            self.$readwrite.operation_mut().set_parameter(
                zstd_safe::CParameter::ContentSizeFlag(include_contentsize),
            )
        }

        /// Enables or disables long-distance matching
        pub fn long_distance_matching(
            &mut self,
            long_distance_matching: bool,
        ) -> io::Result<()> {
            self.$readwrite.operation_mut().set_parameter(
                zstd_safe::CParameter::EnableLongDistanceMatching(
                    long_distance_matching,
                ),
            )
        }

        #[cfg(feature = "experimental")]
        /// Enables or disable the magic bytes at the beginning of each frame.
        ///
        /// If disabled, include_magicbytes must also be called on the decoder.
        ///
        /// Only available with the `experimental` feature.
        pub fn include_magicbytes(
            &mut self,
            include_magicbytes: bool,
        ) -> io::Result<()> {
            self.$readwrite.operation_mut().set_parameter(
                if include_magicbytes {
                    zstd_safe::CParameter::Format(zstd_safe::FrameFormat::One)
                } else {
                    zstd_safe::CParameter::Format(
                        zstd_safe::FrameFormat::Magicless,
                    )
                },
            )
        }
    };
}

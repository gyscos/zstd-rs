// Copyright 2024 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![no_main]

extern crate zstd_safe;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Generate random sized buffer
    let buffer_size = std::cmp::min(data.len() * 2, 2048);
    let mut buffer = vec![0u8; buffer_size];

    // Fuzz compression and decompression
    for level in 0..=20 {
        if let Ok(written) = zstd_safe::compress(&mut buffer[..], data, level) {
            let compressed = &buffer[..written];
            let mut decompressed = vec![0u8; buffer_size];
            let _ = zstd_safe::decompress(&mut decompressed[..], compressed).unwrap_or_else(|_| 0);
        }
    }

    // Fuzz compression and decompression with CCtx
    let mut cctx = zstd_safe::CCtx::default();
    if let Ok(written) = cctx.compress(&mut buffer[..], data, 3) {
        let compressed = &buffer[..written];
        let mut dctx = zstd_safe::DCtx::default();
        let mut decompressed = vec![0u8; buffer_size];
        let _ = dctx.decompress(&mut decompressed[..], compressed).unwrap_or_else(|_| 0);
    }

    // Fuzz compression and decompression on dict
    let dict = b"sample dictionary for zstd fuzzing";
    let mut cctx_dict = zstd_safe::CCtx::default();
    if let Ok(written) = cctx_dict.compress_using_dict(&mut buffer[..], data, dict, 3) {
        let compressed = &buffer[..written];

        let mut dctx_dict = zstd_safe::DCtx::default();
        let mut decompressed = vec![0u8; buffer_size];
        let _ = dctx_dict.decompress_using_dict(&mut decompressed[..], compressed, dict).unwrap_or_else(|_| 0);
    }

    // Fuzz compression and decompression with streaming
    let mut cctx_stream = zstd_safe::CCtx::default();
    let mut dctx_stream = zstd_safe::DCtx::default();
    let mut in_buffer = zstd_safe::InBuffer::around(data);
    let mut out_buffer = zstd_safe::OutBuffer::around(&mut buffer[..]);

    if let Ok(_) = cctx_stream.compress_stream(&mut out_buffer, &mut in_buffer) {
        let mut decompressed_stream = vec![0u8; buffer_size];
        let mut out_buffer_stream = zstd_safe::OutBuffer::around(&mut decompressed_stream[..]);
        let mut in_buffer_stream = zstd_safe::InBuffer::around(out_buffer.as_slice());
        let _ = dctx_stream.decompress_stream(&mut out_buffer_stream, &mut in_buffer_stream).unwrap_or_else(|_| 0);
    }

    // Fuzz error handling and malformed input
    let mut cctx_param = zstd_safe::CCtx::default();
    if let Ok(_) = cctx_param.set_parameter(zstd_safe::CParameter::ChecksumFlag(true)) {
        if let Ok(written) = cctx_param.compress2(&mut buffer[..], data) {
            let compressed = &buffer[..written];
            let mut dctx_param = zstd_safe::DCtx::default();
            let mut decompressed = vec![0u8; buffer_size];
            let _ = dctx_param.decompress(&mut decompressed[..], compressed).unwrap_or_else(|_| 0);
        }
    }
    if let Ok(written) = zstd_safe::compress(&mut buffer[..], data, 3) {
        let compressed = &mut buffer[..written];
        for i in (0..compressed.len()).step_by(5) {
            compressed[i] = compressed[i].wrapping_add(1);
        }

        let mut decompressed = vec![0u8; 2048];
        let mut dctx = zstd_safe::DCtx::default();
        let _ = dctx.decompress(&mut decompressed[..], compressed).unwrap_or_else(|_| 0);
    }
});

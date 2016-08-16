use std::io::{self, Read};

use ll;

struct DecoderContext {
    c: ll::ZBUFFDecompressionContext,
}

impl Default for DecoderContext {
    fn default() -> Self {
        DecoderContext { c: unsafe { ll::ZBUFF_createDCtx() } }
    }
}

impl Drop for DecoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZBUFF_freeDCtx(self.c) };
        ll::parse_code(code).unwrap();
    }
}


/// A decoder that decompress input data from another `Read`.
///
/// This allows to read a stream of compressed data
/// (good for files or heavy network stream).
pub struct Decoder<R: Read> {
    // input reader (compressed data)
    reader: R,
    // input buffer
    buffer: Vec<u8>,
    // we already read everything in the buffer up to that point
    offset: usize,
    // decompression context
    context: DecoderContext,
}

impl<R: Read> Decoder<R> {
    /// Creates a new decoder.
    pub fn new(reader: R) -> io::Result<Self> {
        let context = DecoderContext::default();

        try!(ll::parse_code(unsafe { ll::ZBUFF_decompressInit(context.c) }));

        Decoder::with_context(reader, context)
    }

    /// Creates a new decoder, using an existing dictionary.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_dictionary(reader: R, dictionary: &[u8]) -> io::Result<Self> {
        let context = DecoderContext::default();

        try!(ll::parse_code(unsafe {
            ll::ZBUFF_decompressInitDictionary(context.c,
                                               dictionary.as_ptr(),
                                               dictionary.len())
        }));

        Decoder::with_context(reader, context)
    }

    fn with_context(reader: R, context: DecoderContext) -> io::Result<Self> {
        let buffer_size = unsafe { ll::ZBUFF_recommendedDInSize() };

        Ok(Decoder {
            reader: reader,
            buffer: Vec::with_capacity(buffer_size),
            offset: 0,
            context: context,
        })
    }

    /// Recommendation for the size of the output buffer.
    pub fn recommended_output_size() -> usize {
        unsafe { ll::ZBUFF_recommendedDOutSize() }
    }

    /// Return the inner `Read`.
    ///
    /// Calling `finish()` is not *required* after reading a stream -
    /// just use it if you need to get the `Read` back.
    pub fn finish(self) -> R {
        self.reader
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

        if self.offset > self.buffer.capacity() {
            return Ok(0); // End-of-frame reached.
        }

        let mut written = 0;
        while written != buf.len() {

            if self.offset == self.buffer.len() {
                // We need moar data!
                // Make a nice clean buffer
                let buffer_size = self.buffer.capacity();
                unsafe {
                    self.buffer.set_len(buffer_size);
                }

                // And FILL IT!
                self.offset = 0;
                let read = try!(self.reader.read(&mut self.buffer));
                unsafe {
                    self.buffer.set_len(read);
                }
            }

            let mut out_size = buf.len() - written;
            let mut in_size = self.buffer.len() - self.offset;

            let res = unsafe {
                let code =
                    ll::ZBUFF_decompressContinue(self.context.c,
                                                 buf[written..].as_mut_ptr(),
                                                 &mut out_size,
                                                 self.buffer[self.offset..]
                                                     .as_ptr(),
                                                 &mut in_size);
                try!(ll::parse_code(code))
            };

            written += out_size;
            if res == 0 {
                // End-of-frame marker.
                self.offset = self.buffer.capacity() + 1;
                break;
            }
            self.offset += in_size;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_read_small() {
        use std::io::{self, Read};
        use super::Decoder;

        const EXPECTED_DECODED_LENGTH: usize = 18;
        let mut output = [0u8; EXPECTED_DECODED_LENGTH];
        let input = &[
            0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x68, 0x7d, 0x00,
            0x00, 0x48, 0x05, 0x54, 0x45, 0x53, 0x54, 0x30,
            0x00, 0x10, 0x00, 0x01, 0x00, 0x05, 0x38, 0x02,
        ];

        let r = io::Cursor::new(input);
        let mut r = Decoder::new(r).unwrap();

        assert!(r.read(&mut output[..1]).unwrap() == 1);
        assert!(r.read(&mut output[1..]).unwrap() > 0);
    }
}

use futures::{executor, Future, Poll};
use partial_io::{
    GenInterruptedWouldBlock, PartialAsyncWrite, PartialWithErrors,
};
use quickcheck::quickcheck;
use std::io::{self, Cursor};
use tokio_io::{AsyncRead, AsyncWrite};

#[test]
fn test_async_write() {
    use crate::stream::decode_all;

    let source = "abc".repeat(1024 * 100).into_bytes();
    let encoded_output =
        test_async_write_worker(&source[..], Cursor::new(Vec::new()), |w| {
            w.into_inner()
        });
    let decoded = decode_all(&encoded_output[..]).unwrap();
    assert_eq!(source, &decoded[..]);
}

#[test]
fn test_async_write_partial() {
    quickcheck(test as fn(_) -> _);

    fn test(encode_ops: PartialWithErrors<GenInterruptedWouldBlock>) {
        use crate::stream::decode_all;

        let source = "abc".repeat(1024 * 100).into_bytes();
        let writer =
            PartialAsyncWrite::new(Cursor::new(Vec::new()), encode_ops);
        let encoded_output =
            test_async_write_worker(&source[..], writer, |w| {
                w.into_inner().into_inner()
            });
        let decoded = decode_all(&encoded_output[..]).unwrap();
        assert_eq!(source, &decoded[..]);
    }
}

struct Finish<W: AsyncWrite> {
    encoder: Option<super::Encoder<'static, W>>,
}

impl<W: AsyncWrite> Future for Finish<W> {
    type Item = W;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<W, io::Error> {
        use futures::Async;

        match self.encoder.take().unwrap().try_finish() {
            Ok(v) => return Ok(v.into()),
            Err((encoder, err)) => {
                if err.kind() == io::ErrorKind::WouldBlock {
                    self.encoder = Some(encoder);
                    return Ok(Async::NotReady);
                } else {
                    return Err(err);
                }
            }
        };
    }
}

fn test_async_write_worker<
    R: AsyncRead,
    W: AsyncWrite,
    Res,
    F: FnOnce(W) -> Res,
>(
    r: R,
    w: W,
    f: F,
) -> Res {
    use super::Encoder;

    let encoder = Encoder::new(w, 1).unwrap();
    let copy_future = tokio_io::io::copy(r, encoder)
        .and_then(|(_, _, encoder)| Finish {
            encoder: Some(encoder),
        })
        .map(f);
    executor::spawn(copy_future).wait_future().unwrap()
}

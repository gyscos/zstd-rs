use futures::Future;
use partial_io::{GenWouldBlock, PartialAsyncRead, PartialWithErrors};
use quickcheck::quickcheck;
use std::io::{self, Cursor};
use tokio_io::{AsyncRead, AsyncWrite};

#[test]
fn test_async_read() {
    use crate::stream::encode_all;

    let source = "abc".repeat(1024 * 10).into_bytes();
    let encoded = encode_all(&source[..], 1).unwrap();
    let writer =
        test_async_read_worker(&encoded[..], Cursor::new(Vec::new())).unwrap();
    let output = writer.into_inner();
    assert_eq!(source, output);
}

#[test]
fn test_async_read_partial() {
    quickcheck(test as fn(_) -> _);

    // This used to test for Interrupted errors as well.
    // But right now a solution to silently ignore Interrupted error
    // would not compile.
    // Plus, it's still not clear it's a good idea.
    fn test(encode_ops: PartialWithErrors<GenWouldBlock>) {
        use crate::stream::encode_all;

        let source = "abc".repeat(1024 * 10).into_bytes();
        let encoded = encode_all(&source[..], 1).unwrap();
        let reader = PartialAsyncRead::new(&encoded[..], encode_ops);
        let writer =
            test_async_read_worker(reader, Cursor::new(Vec::new())).unwrap();
        let output = writer.into_inner();
        assert_eq!(source, output);
    }
}

fn test_async_read_worker<R: AsyncRead, W: AsyncWrite>(
    r: R,
    w: W,
) -> io::Result<W> {
    use super::Decoder;

    let decoder = Decoder::new(r).unwrap();
    let (_, _, w) = tokio_io::io::copy(decoder, w).wait()?;
    Ok(w)
}

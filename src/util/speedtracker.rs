use std::{
    marker::PhantomPinned,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, ready},
};

use pin_project_lite::pin_project;
use tokio::io::{self, AsyncRead, AsyncReadExt, ReadBuf};

pub static TOTAL_IN: AtomicUsize = AtomicUsize::new(0);

pub fn read_payload<'a>(
    stream: &'a mut (impl AsyncReadExt + Unpin),
    buffer: &'a mut [u8],
) -> ReadPayloadBytes<'a, impl AsyncReadExt + Unpin> {
    // Read the entire packet into buffer
    ReadPayloadBytes {
        reader: stream,
        buf: ReadBuf::new(buffer),
        _pin: PhantomPinned,
    }
}

pin_project! {
    pub struct ReadPayloadBytes<'a, A: ?Sized> {
        reader: &'a mut A,
        buf: ReadBuf<'a>,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<A> Future for ReadPayloadBytes<'_, A>
where
    A: AsyncRead + Unpin + ?Sized,
{
    type Output = io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let me = self.project();

        loop {
            // if our buffer is empty, then we need to read some data to continue.
            let rem = me.buf.remaining();
            if rem != 0 {
                ready!(Pin::new(&mut *me.reader).poll_read(cx, me.buf))?;
                if me.buf.remaining() == rem {
                    return Err(eof()).into();
                }
                TOTAL_IN.fetch_add(rem - me.buf.remaining(), Ordering::Relaxed);
            } else {
                return Poll::Ready(Ok(me.buf.capacity()));
            }
        }
    }
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "early eof")
}

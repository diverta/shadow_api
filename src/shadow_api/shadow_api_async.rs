use std::{task::{Poll, Context}, rc::Rc, cell::RefCell, pin::Pin};

use futures::AsyncWrite;
use lol_html::{Settings, HtmlRewriter, OutputSink};

pub struct LoLOutputter {
    done: Rc<RefCell<bool>>,
    //waker: Rc<Waker>,
    buffer: Rc<RefCell<Vec<u8>>>,
    no_output: bool,
}

impl OutputSink for LoLOutputter {
    fn handle_chunk(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            *self.done.borrow_mut() = true;
        } else if !self.no_output {
            self.buffer.borrow_mut().extend(chunk.to_vec());
        }
    }
}

pub struct ShadowApiRewriterAsync<'h, W> {
    buffer: Rc<RefCell<Vec<u8>>>,
    rewriter: HtmlRewriter<'h, LoLOutputter>,
    writer: Pin<&'h mut W>,
    no_output: bool
}

impl<'h, W> ShadowApiRewriterAsync<'h, W>
where
    W: AsyncWrite + Unpin
{
    /// If 'no_output' is set to true, LolHtml processing will still apply on the input, but the output won't be written
    pub fn new(
        settings: Settings<'h, '_>,
        writer: Pin<&'h mut W>,
        no_output: bool,
    ) -> Self {
        //let waker = Rc::new(Waker::new());
        let done = Rc::new(RefCell::new(false));
        let buffer: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        
        let output_sink = LoLOutputter {
            //waker: waker.clone(),
            done: Rc::clone(&done),
            buffer: Rc::clone(&buffer),
            no_output,
        };

        let rewriter: HtmlRewriter<'_, LoLOutputter> = HtmlRewriter::new(settings, output_sink);

        Self {
            buffer,
            rewriter,
            writer,
            no_output,
        }
    }
}

impl<'h, W> AsyncWrite for ShadowApiRewriterAsync<'h, W>
where
    W: AsyncWrite + Unpin
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // TODO: rewrite buf chunk
        if let Err(err) = self.rewriter.write(buf) {
            return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, format!("[HtmlRewriterError] {}", err))));
        };
        let buffer_rc = Rc::clone(&self.buffer);
        let mut buffer = buffer_rc.borrow_mut();
        if self.no_output {
            Poll::Ready(Ok(0))
        } else {
            match AsyncWrite::poll_write(Pin::new(&mut self.writer), cx, &buffer) {
                Poll::Ready(done) => {
                    // Buffered data dumped
                    buffer.clear();
                    Poll::Ready(done)
                },
                Poll::Pending => Poll::Pending,
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        AsyncWrite::poll_flush(Pin::new(&mut self.writer), cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        AsyncWrite::poll_close(Pin::new(&mut self.writer), cx)
    }
}
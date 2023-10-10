use std::{task::{Poll, Context}, rc::Rc, cell::RefCell, pin::Pin};
use pin_project_lite::pin_project;

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

pin_project! {
    pub struct ShadowApiRewriterAsync<'h, W> {
        buffer: Rc<RefCell<Vec<u8>>>,
        rewriter: HtmlRewriter<'h, LoLOutputter>,
        #[pin]
        writer: &'h mut W,
        no_output: bool,
        is_write_pending: bool // If the previous poll_write returned Pending, then we don't want to write any more - so this flag helps tracking the state
    }
}

impl<'h, W> ShadowApiRewriterAsync<'h, W>
where
    W: AsyncWrite + Unpin
{
    /// If 'no_output' is set to true, LolHtml processing will still apply on the input, but the output won't be written
    pub fn new(
        settings: Settings<'h, '_>,
        writer: &'h mut W,
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
            is_write_pending: false
        }
    }
}

impl<'h, W> AsyncWrite for ShadowApiRewriterAsync<'h, W>
where
    W: AsyncWrite + Unpin
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        if !*this.is_write_pending {
            if let Err(err) = this.rewriter.write(buf) {
                return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, format!("[HtmlRewriterError] {}", err))));
            };
        }
        if *this.no_output {
            Poll::Ready(Ok(0))
        } else {
            let buffer_rc = Rc::clone(&this.buffer);
            let mut buffer = buffer_rc.borrow_mut();
            match this.writer.poll_write(cx, &buffer) {
                Poll::Ready(done) => {
                    *this.is_write_pending = false;
                    // Buffered data dumped
                    buffer.clear();
                    Poll::Ready(done)
                },
                Poll::Pending => {
                    *this.is_write_pending = true;
                    Poll::Pending
                },
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
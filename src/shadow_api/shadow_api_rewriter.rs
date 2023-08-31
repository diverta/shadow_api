use std::io;
use lol_html::{OutputSink, HtmlRewriter, errors::RewritingError};


pub struct ShadowApiRewriter<'a, O: OutputSink> {
    pub rewriter: HtmlRewriter<'a, O>
}

impl<'a, O: OutputSink> ShadowApiRewriter<'a, O> {
    pub fn new(rewriter: HtmlRewriter<'a, O>) -> Self {
        Self { rewriter }
    }

    pub fn end(self) -> Result<(), RewritingError> {
        self.rewriter.end()
    }
}


impl<'a, O: OutputSink> AsMut<HtmlRewriter<'a, O>> for ShadowApiRewriter<'a, O> {
    fn as_mut(&mut self) -> &mut HtmlRewriter<'a, O> {
        &mut self.rewriter
    }
}

impl<O: OutputSink> io::Write for ShadowApiRewriter<'_, O> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.rewriter.write(buf) {
            Ok(_) => Ok(buf.len()),
            Err(e) => Err(
                std::io::Error::new(
                    io::ErrorKind::Interrupted, e.to_string()
                )
            ),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        //self.as_mut().end().map_err(|err| std::io::Error::new(io::ErrorKind::Interrupted, err.to_string())); // todo
        Ok(())
    }
}
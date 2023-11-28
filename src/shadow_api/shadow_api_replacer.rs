use std::{cell::RefCell, rc::Rc};
use lol_html::{HtmlRewriter, errors::RewritingError, Settings};


pub struct ShadowApiReplacer<'h> {
    pub rewriter: HtmlRewriter<'h, Box<dyn FnMut(&[u8])>>,
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub write_idx: Rc<RefCell<usize>>
}

impl<'h> ShadowApiReplacer<'h> {
    pub fn new<'s>(settings: Settings<'h, 's>) -> Self {
        let buffer: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let write_idx: Rc<RefCell<usize>> = Rc::new(RefCell::new(0usize));
        let buffer_to_move = Rc::clone(&buffer);
        let write_idx_to_move = Rc::clone(&write_idx);
        let rewriter: HtmlRewriter<'h, Box<dyn FnMut(&[u8])>> = HtmlRewriter::new(
            settings,
            Box::new(move |c: &[u8]| {
                let mut buffer_ref = buffer_to_move.borrow_mut();
                let mut write_idx_borrowed = write_idx_to_move.borrow_mut();
                if buffer_ref.len() < *write_idx_borrowed + c.len() {
                    buffer_ref.resize(*write_idx_borrowed + c.len(), b'\0');
                }
                let initial_pos = *write_idx_borrowed;
                *write_idx_borrowed += c.len();
                buffer_ref[initial_pos..*write_idx_borrowed].copy_from_slice(c)
            })
        );
        Self {
            rewriter,
            buffer,
            write_idx,
        }
    }

    /// Writes data in the internal buffer. Written amount of bytes is returned along with the reference to the buffer
    /// Make sure to read only the amount of bytes written
    pub fn replace(&mut self, chunk: &[u8]) -> Result<(Rc<RefCell<Vec<u8>>>, usize), RewritingError> {
        self.rewriter.write(chunk)?;
        let mut write_idx = self.write_idx.borrow_mut();
        if *write_idx > 0 {
            let written = *write_idx;
            *write_idx = 0; // Reset becase we are about to consume it
            Ok((Rc::clone(&self.buffer), written))
        } else {
            Ok((Rc::clone(&self.buffer), 0))
        }
    }

    pub fn finish(self) -> Result<(), RewritingError> {
        self.rewriter.end()
    }
}
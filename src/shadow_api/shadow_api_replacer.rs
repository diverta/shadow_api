use std::{cell::RefCell, rc::Rc};
use lol_html::{HtmlRewriter, errors::RewritingError, Settings};


pub struct ShadowApiReplacer<'h> {
    pub rewriter: HtmlRewriter<'h, Box<dyn FnMut(&[u8])>>,
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub written: Rc<RefCell<usize>>,
}

impl<'h> ShadowApiReplacer<'h> {
    pub fn new<'s>(settings: Settings<'h, 's>) -> Self {
        let buffer: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let written: Rc<RefCell<usize>> = Rc::new(RefCell::new(0usize));
        let buffer_to_move = Rc::clone(&buffer);
        let written_to_move = Rc::clone(&written);
        let rewriter: HtmlRewriter<'h, Box<dyn FnMut(&[u8])>> = HtmlRewriter::new(
            settings,
            Box::new(move |c: &[u8]| {
                let mut buffer_ref = buffer_to_move.borrow_mut();
                if buffer_ref.len() < c.len() {
                    buffer_ref.resize(c.len(), b'\0');
                }
                *written_to_move.borrow_mut() = c.len();
                buffer_ref[..c.len()].copy_from_slice(c)
            })
        );
        Self {
            rewriter,
            buffer,
            written
        }
    }

    pub fn replace(&mut self, chunk: &[u8]) -> Result<(Rc<RefCell<Vec<u8>>>, Rc<RefCell<usize>>), RewritingError> {
        self.rewriter.write(chunk)?;
        Ok((Rc::clone(&self.buffer), Rc::clone(&self.written)))
    }

    pub fn replace_owned(&mut self, chunk: Vec<u8>) -> Result<(Rc<RefCell<Vec<u8>>>, Rc<RefCell<usize>>), RewritingError> {
        self.rewriter.write(&chunk)?;
        Ok((Rc::clone(&self.buffer), Rc::clone(&self.written)))
    }

    pub fn finish(self) -> Result<(), RewritingError> {
        self.rewriter.end()
    }
}
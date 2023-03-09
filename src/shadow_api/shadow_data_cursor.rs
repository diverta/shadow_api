use core::fmt;
use std::{rc::{Weak, Rc}, cell::RefCell};

use crate::ShadowData;

use super::ShadowError;

#[derive(Debug)]
pub struct ShadowDataCursor {
    pub root: Rc<RefCell<ShadowData>>,
    pub shadow_data: Rc<RefCell<ShadowData>>
}


impl fmt::Display for ShadowDataCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root.borrow().to_string())
    }
}

impl ShadowDataCursor {
    pub fn new(shadow_data: Rc<RefCell<ShadowData>>, root: Rc<RefCell<ShadowData>>) -> ShadowDataCursor {
        ShadowDataCursor { root, shadow_data }
    }
    pub fn init() -> ShadowDataCursor {
        let new_shadow_data = ShadowData::wrap(ShadowData::new_object(Some(0), Weak::new()));
        ShadowDataCursor { root: Rc::clone(&new_shadow_data), shadow_data: new_shadow_data }
    }
    // Print tree structure, for debugging
    pub fn visualize(&self) -> String {
        self.root.borrow().visualize(0)
    }
    pub fn go_up(&mut self) -> Result<(), ShadowError> {
        // If a path is defined, then a new nested element must had been added => go up the tree once
        let parent_weak = Weak::clone(&self.shadow_data.borrow().parent);
        if let Some(parent) = parent_weak.upgrade() {
            *self = ShadowDataCursor::new(parent, Rc::clone(&self.root));
        } else {
            return Err(ShadowError {
                msg: format!("[go_up] cannot move up")
            });
        }
        Ok(())
    }
}
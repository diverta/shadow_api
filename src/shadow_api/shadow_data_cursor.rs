use core::fmt;
use std::{rc::{Weak, Rc}, cell::RefCell};

use crate::ShadowData;

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
}
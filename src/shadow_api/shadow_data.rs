use core::fmt;
use std::{cell::RefCell, rc::Rc};

use indexmap::IndexMap;

// ShadowData is a minimalistic tree structure representing json value which contains only Objects, Arrays or Strings, wrapped in Rc<RefCell<T>>
// The reason we don't use serde::json for this is that while serde::json is able to deserialize into Rc (through a feature), RefCells are not supported
#[derive(Debug)]
pub enum ShadowData {
    String(Rc<RefCell<String>>),
    Array(Vec<Rc<RefCell<ShadowData>>>),
    Object(IndexMap<String, Rc<RefCell<ShadowData>>>)
}

impl Default for ShadowData {
    fn default() -> Self {
        ShadowData::new_string("".to_string())
    }
}

impl fmt::Display for ShadowData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShadowData::String(v) => {
                let val = v.borrow();
                let parsed: String = serde_json::from_str(&format!("\"{}\"", val)).unwrap_or(String::from("")); // Using serde to escape the value
                write!(f, "\"{}\"", parsed)
            },
            ShadowData::Array(v) => {
                write!(f, "[{}]", v.iter().fold(String::new(), |mut carry, x| {
                    let borrowed = x.borrow();
                    if carry.len() != 0 {
                        carry += ",";
                    }
                    carry += &borrowed.to_string();
                    carry
                }))
            },
            ShadowData::Object(v) => {
                write!(f, "{{{}}}", v.into_iter().fold(String::new(), |mut carry, (key,  value)| {
                    let borrowed = value.borrow();
                    if carry.len() != 0 {
                        carry += ",";
                    }
                    format!("{}\"{}\":{}", carry, key, borrowed.to_string()).as_str().to_string()
                }))
            },
        }
    }
}

impl ShadowData {
    pub fn wrap(s: Self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(s))
    }
    pub fn new_string(v: String) -> Self {
        return ShadowData::String(Rc::new(RefCell::new(v)));
    }
    pub fn new_array() -> Self {
        return ShadowData::Array(Vec::new());
    }
    pub fn new_object() -> Self {
        return ShadowData::Object(IndexMap::new());
    }
    pub fn is_string(&self) -> bool {
        return match &self {
            Self::String(_) => true,
            _ => false
        }
    }
    pub fn as_string(&self) -> Option<Rc<RefCell<String>>> {
        return match &self {
            Self::String(s) => Some(Rc::clone(s)),
            _ => None
        }
    }
    pub fn is_array(&self) -> bool {
        return match &self {
            Self::Array(_) => true,
            _ => false
        }
    }
    pub fn as_array(&self) -> Option<&Vec<Rc<RefCell<Self>>>> {
        return match &self {
            Self::Array(s) => Some(s),
            _ => None
        }
    }
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Rc<RefCell<Self>>>> {
        return match self {
            Self::Array(s) => Some(s),
            _ => None
        }
    }
    pub fn is_object(&self) -> bool {
        return match &self {
            Self::Object(_) => true,
            _ => false
        }
    }
    pub fn as_object(&self) -> Option<&IndexMap<String, Rc<RefCell<Self>>>> {
        return match &self {
            Self::Object(s) => Some(s),
            _ => None
        }
    }
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Rc<RefCell<Self>>>> {
        return match self {
            Self::Object(s) => Some(s),
            _ => None
        }
    }
    pub fn get(&self, key: &str) -> Option<Rc<RefCell<ShadowData>>> {
        match self {
            ShadowData::String(_) => panic!("ShadowData::set cannot be applied on String subtype"),
            ShadowData::Array(_) => panic!("ShadowData::set cannot be applied on Array subtype"),
            ShadowData::Object(o) => {
                if let Some(val) = o.get(key) {
                    Some(Rc::clone(val))
                } else {
                    None
                }
            }
        }
    }
    // Merges map2 into map1
    pub fn merge(map1: &mut IndexMap<String, Rc<RefCell<ShadowData>>>, map2: &mut IndexMap<String, Rc<RefCell<ShadowData>>>) {
        for (subkey, map2_subval) in map2 {
            let map2_rc = Rc::clone(map2_subval);
            let map1_subval_opt = map1.get_mut(subkey);
            if let Some(map1_subval) = map1_subval_opt {
                let both_objects: bool;
                {
                    let map1_subval_borrowed = map1_subval.borrow();
                    let map2_subval_borrowed = map2_rc.borrow();
                    both_objects = match (&*map1_subval_borrowed, &*map2_subval_borrowed) {
                        (ShadowData::Object(_), ShadowData::Object(_)) => true,
                        _ => false
                    }
                }
                if both_objects {
                    // Recursive merge
                    let mut map1_subval_borrowed = map1_subval.borrow_mut();
                    let mut map2_subval_borrowed = map2_subval.borrow_mut();
                    let map1_subval_borrowed = map1_subval_borrowed.as_object_mut().unwrap();
                    let map2_subval_borrowed = map2_subval_borrowed.as_object_mut().unwrap();
                    Self::merge(map1_subval_borrowed, map2_subval_borrowed);
                } else {
                    // Overriding the meaningful map1 value by map2 by repointing to the relevant data of map2
                    map1_subval.swap(map2_subval);
                }
            } else {
                map1.insert(subkey.clone(), Rc::clone(map2_subval));
            }
        }
    }
    pub fn set(&mut self, key: &str, val: Rc<RefCell<ShadowData>>) {
        match self {
            ShadowData::String(_) => panic!("ShadowData::set cannot be applied on String subtype"),
            ShadowData::Array(_) => panic!("ShadowData::set cannot be applied on Array subtype"),
            ShadowData::Object(ref mut o) => {
                let existing_key_opt = o.get_mut(key);
                if let Some(existing_key_rc) = existing_key_opt {
                    // Data found at this key => merge
                    let mut existing_key_borrowed = existing_key_rc.borrow_mut();
                    let mut override_flag = false;
                    match &mut *existing_key_borrowed {
                        ShadowData::String(_)
                        | ShadowData::Array(_) => {
                            // A case where user definition writes into non-object key => override (avoid panic)
                            override_flag = true;
                        },
                        ShadowData::Object(sub_o) => {
                            // self is an object => if val is an object too, merge . if val is not an object, override yet again
                            let val_rc = Rc::clone(&val);
                            let mut val_borrowed = val_rc.borrow_mut();
                            match &mut *val_borrowed {
                                ShadowData::String(_)
                                | ShadowData::Array(_) => {
                                    override_flag = true;
                                },
                                ShadowData::Object(val_object) => {
                                    // Merging two objects
                                    Self::merge(sub_o, val_object);
                                },
                            }
                        },
                    }
                    if override_flag {
                        *existing_key_borrowed = val.take(); // val's contents get swapped out, as they will now belong to the current structure
                    }
                } else {
                    // There is no data in the object at this key
                    o.insert(key.to_string(), Rc::clone(&val));
                }
            }
        }
    }
    pub fn push(&mut self, val: Rc<RefCell<ShadowData>>) {
        match self {
            ShadowData::String(_) => panic!("ShadowData::push cannot be applied on String subtype"),
            ShadowData::Array(ref mut o) => {
                o.push(Rc::clone(&val));
            }
            ShadowData::Object(_) => panic!("ShadowData::push cannot be applied on Object subtype. Self : {:#?} Val: {:#?}", self, val),
        }
    }
    // Force conversion of data_orig into object, by pushing a new element into the array if it is one
    pub fn force_object(data_orig: Rc<RefCell<ShadowData>>) -> Option<Rc<RefCell<ShadowData>>> {
        let rc_data_orig = Rc::clone(&data_orig);
        let mut borrowed = rc_data_orig.borrow_mut();
        match *borrowed {
            ShadowData::String(_) => {
                panic!("ShadowData::get_map_mut.force_object is neither object nor array. Program bug");
            },
            ShadowData::Array(ref mut data) => {
                let new_data = ShadowData::wrap(ShadowData::new_object());
                data.push(Rc::clone(&new_data));
                Some(new_data)
            },
            ShadowData::Object(_) => None, // Perfect as-is
        }
    }
    pub fn transform_strings(&mut self, f: &dyn Fn(&mut String)) {
        match self {
            ShadowData::String(s) => {
                f(&mut s.borrow_mut());
            },
            ShadowData::Array(arr) => {
                arr.iter().for_each(|a| {
                    // Cannot change keys (would require removing and reinserting new). Don't do for now
                    a.borrow_mut().transform_strings(f);
                });
            },
            ShadowData::Object(obj) => {
                obj.iter().for_each(|a| {
                    // Cannot change keys (would require removing and reinserting new). Don't do for now
                    a.1.borrow_mut().transform_strings(f);
                });
            },
        }
    }
}
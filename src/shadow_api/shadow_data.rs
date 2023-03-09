use core::fmt;
use rand::prelude::*;
use std::{cell::RefCell, rc::{Rc, Weak}};

use indexmap::IndexMap;
use lol_html::html_content::{Element, EndTag};

use crate::{ShadowJson, ShadowDataCursor};

use super::{ShadowError};

// ShadowData is a minimalistic tree structure representing json value which contains only Objects, Arrays or Strings, wrapped in Rc<RefCell<T>>
// The reason we don't use serde::json for this is that while serde::json is able to deserialize into Rc (through a feature), RefCells are not supported
#[derive(Debug)]
pub struct ShadowData {
    pub id: Option<usize>, // Selector identifier
    pub parent: Weak<RefCell<ShadowData>>,
    pub v: ShadowDataValue,
    pub uid: String, // Unique data element identified
}

#[derive(Debug)]
pub enum ShadowDataValue {
    String(Rc<RefCell<String>>),
    Array(Vec<Rc<RefCell<ShadowData>>>),
    Object(IndexMap<String, Rc<RefCell<ShadowData>>>)
}

impl Default for ShadowData {
    fn default() -> Self {
        ShadowData {
            id: None,
            parent: Weak::new(),
            uid: String::from("0_0000"),
            v: ShadowDataValue::String(Rc::new(RefCell::new(String::new())))
        }
    }
}

impl fmt::Display for ShadowData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.v {
            ShadowDataValue::String(v) => {
                let val = v.borrow();
                let parsed: String = serde_json::from_str(&format!("\"{}\"", val)).unwrap_or(String::from("")); // Using serde to escape the value
                write!(f, "\"{}\"", parsed)
            },
            ShadowDataValue::Array(v) => {
                write!(f, "[{}]", v.iter().fold(String::new(), |mut carry, x| {
                    let borrowed = x.borrow();
                    if carry.len() != 0 {
                        carry += ",";
                    }
                    carry += &borrowed.to_string();
                    carry
                }))
            },
            ShadowDataValue::Object(v) => {
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
    fn uid(id: Option<usize>) -> String {
        // Pseudo random internal id for el identification
        let mut nums: Vec<i32> = (1000..9999).collect();
        nums.shuffle(&mut rand::thread_rng());
        format!("{}_{}", id.unwrap_or(0), nums.first().unwrap())
    }
    pub fn new_string(id: Option<usize>, parent: Weak<RefCell<ShadowData>>, v: String) -> Self {
        return ShadowData { id, parent, uid: Self::uid(id), v: ShadowDataValue::String(Rc::new(RefCell::new(v))) };
    }
    pub fn new_array(id: Option<usize>, parent: Weak<RefCell<ShadowData>>) -> Self {
        return ShadowData { id, parent, uid: Self::uid(id), v: ShadowDataValue::Array(Vec::new()) };
    }
    pub fn new_object(id: Option<usize>, parent: Weak<RefCell<ShadowData>>) -> Self {
        return ShadowData { id, parent, uid: Self::uid(id), v: ShadowDataValue::Object(IndexMap::new()) };
    }
    pub fn is_string(&self) -> bool {
        return match &self.v {
            ShadowDataValue::String(_) => true,
            _ => false
        }
    }
    pub fn as_string(&self) -> Option<Rc<RefCell<String>>> {
        return match &self.v {
            ShadowDataValue::String(s) => Some(Rc::clone(s)),
            _ => None
        }
    }
    pub fn is_array(&self) -> bool {
        return match &self.v {
            ShadowDataValue::Array(_) => true,
            _ => false
        }
    }
    pub fn as_array(&self) -> Option<&Vec<Rc<RefCell<Self>>>> {
        return match &self.v {
            ShadowDataValue::Array(s) => Some(s),
            _ => None
        }
    }
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Rc<RefCell<Self>>>> {
        return match &mut self.v {
            ShadowDataValue::Array(s) => Some(s),
            _ => None
        }
    }
    pub fn is_object(&self) -> bool {
        return match &self.v {
            ShadowDataValue::Object(_) => true,
            _ => false
        }
    }
    pub fn as_object(&self) -> Option<&IndexMap<String, Rc<RefCell<Self>>>> {
        return match &self.v {
            ShadowDataValue::Object(s) => Some(s),
            _ => None
        }
    }
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Rc<RefCell<Self>>>> {
        return match &mut self.v {
            ShadowDataValue::Object(s) => Some(s),
            _ => None
        }
    }
    pub fn get(&self, key: &str) -> Option<Rc<RefCell<ShadowData>>> {
        match &self.v {
            ShadowDataValue::String(_) => panic!("ShadowData::get cannot be applied on String subtype"),
            ShadowDataValue::Array(_) => panic!("ShadowData::get cannot be applied on Array subtype"),
            ShadowDataValue::Object(o) => {
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
                    let map1_subval_borrowed = &map1_subval.borrow().v;
                    let map2_subval_borrowed = &map2_rc.borrow().v;
                    both_objects = match (map1_subval_borrowed, map2_subval_borrowed) {
                        (ShadowDataValue::Object(_), ShadowDataValue::Object(_)) => true,
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
        match &mut self.v {
            ShadowDataValue::String(_) => panic!("ShadowData::set cannot be applied on String subtype"),
            ShadowDataValue::Array(_) => panic!("ShadowData::set cannot be applied on Array subtype"),
            ShadowDataValue::Object(ref mut o) => {
                let existing_key_opt = o.get_mut(key);
                if let Some(existing_key_rc) = existing_key_opt {
                    // Data found at this key => merge
                    let mut existing_key_borrowed = existing_key_rc.borrow_mut();
                    let mut override_flag = false;
                    match &mut existing_key_borrowed.v {
                        ShadowDataValue::String(_)
                        | ShadowDataValue::Array(_) => {
                            // A case where user definition writes into non-object key => override (avoid panic)
                            override_flag = true;
                        },
                        ShadowDataValue::Object(sub_o) => {
                            // self is an object => if val is an object too, merge . if val is not an object, override yet again
                            let val_rc = Rc::clone(&val);
                            let mut val_borrowed = val_rc.borrow_mut();
                            match &mut val_borrowed.v {
                                ShadowDataValue::String(_)
                                | ShadowDataValue::Array(_) => {
                                    override_flag = true;
                                },
                                ShadowDataValue::Object(val_object) => {
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
        match self.v {
            ShadowDataValue::String(_) => panic!("ShadowData::push cannot be applied on String subtype"),
            ShadowDataValue::Array(ref mut o) => {
                o.push(Rc::clone(&val));
            }
            ShadowDataValue::Object(_) => panic!("ShadowData::push cannot be applied on Object subtype. Self : {:#?} Val: {:#?}", self, val),
        }
    }
    // Force conversion of data_orig into object, by pushing a new element into the array if it is one
    pub fn force_object(data_orig: Rc<RefCell<ShadowData>>) -> Option<Rc<RefCell<ShadowData>>> {
        let rc_data_orig = Rc::clone(&data_orig);
        let mut borrowed = rc_data_orig.borrow_mut();
        let parent = Weak::clone(&borrowed.parent);
        let id = borrowed.id;
        match borrowed.v {
            ShadowDataValue::String(_) => {
                panic!("ShadowData::get_map_mut.force_object is neither object nor array. Program bug");
            },
            ShadowDataValue::Array(ref mut data) => {
                let new_data = ShadowData::wrap(ShadowData::new_object(id, parent));
                data.push(Rc::clone(&new_data));
                Some(new_data)
            },
            ShadowDataValue::Object(_) => None, // Perfect as-is
        }
    }
    pub fn transform_strings(&mut self, f: &dyn Fn(&mut String)) {
        match &self.v {
            ShadowDataValue::String(s) => {
                f(&mut s.borrow_mut());
            },
            ShadowDataValue::Array(arr) => {
                arr.iter().for_each(|a| {
                    // Cannot change keys (would require removing and reinserting new). Don't do for now
                    a.borrow_mut().transform_strings(f);
                });
            },
            ShadowDataValue::Object(obj) => {
                obj.iter().for_each(|a| {
                    // Cannot change keys (would require removing and reinserting new). Don't do for now
                    a.1.borrow_mut().transform_strings(f);
                });
            },
        }
    }

    // Returns the current Cell 
    pub fn on_data_tag_open(
        _el: &mut Element,
        selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        cursor: Rc<RefCell<ShadowDataCursor>>
    ) -> Result<Option<Rc<RefCell<ShadowData>>>, ShadowError> {
        if let Some(data_def) = json_def.borrow().data.as_ref() {
            let path = data_def.path.clone();
            let mut cursor = cursor.borrow_mut();

            let is_current = {
                cursor.shadow_data.borrow_mut().id
                    .and_then(|cur_id| {
                        Some(cur_id == selector_id)
                    })
                    .unwrap_or(false)
            };
            let is_current_an_array = {
                cursor.shadow_data.borrow_mut().is_array()
            };

            if !is_current && is_current_an_array {
                // A sibling found along with an iterating array. This case SHOULD only happen if previous sibling defined an array path
                // Cursor is now pointing at the array of the previous sibling, so we want to go up once
                cursor.go_up()?;
            }

            if let Some(mut path) = path {
                // A path is specified => we need to create (or reuse) a deeper element, and overwrite next_data
                let mut is_array = false;
                if path.chars().last().unwrap() == '.' {
                    // Determine whether this element is part of an array of elements
                    is_array = true;
                    path = (path[..path.len() - 1]).to_string(); // Remove the last dot

                    if path.len() == 0 {
                        return Err(ShadowError {
                            msg: "Invalid def : single dot is not a valid path".to_string()
                        });
                    }
                }

                let mut split = path.split('.').peekable();
                let mut current_data = Rc::clone(&cursor.shadow_data);

                // The contents of this Cell will be changed once we can determine the parent
                let parent = Rc::downgrade(&current_data);
                while let Some(word) = split.next() {
                    let current_data_c = Rc::clone(&current_data);
                    let current_ref = Rc::clone(&current_data);
                    if split.peek().unwrap_or(&"").len() == 0 { // Found last word
                        // Here, we either build a new nested object or fetch an existing one, and assign it to next_data for further processing
                        if is_array {
                            let data_array = if is_current && is_current_an_array {
                                // Case of coming back to non-first detected element for this selector => Simply reuse current_data
                                current_data_c
                            } else {
                                // Case when a new array needs to be built at the given path (ending with dot)
                                let mut temp_data = current_data_c.borrow_mut();
                                match temp_data.get(word) {
                                    Some(existing_el) => {
                                        let existing_el_rc = Rc::clone(&existing_el);
                                        let array_el = match existing_el_rc.borrow().v {
                                            ShadowDataValue::String(_) | ShadowDataValue::Object(_) => {
                                                let new_array = ShadowData::wrap(
                                                    ShadowData::new_array(Some(selector_id), Rc::downgrade(&current_ref)
                                                ));
                                                temp_data.set(word, Rc::clone(&new_array));
                                                new_array
                                            },
                                            ShadowDataValue::Array(_) => existing_el
                                        };
                                        array_el
                                    },
                                    None => {
                                        let array_el = ShadowData::wrap(
                                            ShadowData::new_array(Some(selector_id), Rc::downgrade(&current_ref)
                                        ));
                                        temp_data.set(word, Rc::clone(&array_el));
                                        array_el
                                    }
                                }
                            };
                            let parent_array = Rc::downgrade(&data_array); // Creating weak reference to parent array
                            let new_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), parent_array));
                            *cursor = ShadowDataCursor::new(Rc::clone(&new_data), Rc::clone(&cursor.root)); // Next data is now pointing to the first (empty) object of the array
                            data_array.borrow_mut().push(Rc::clone(&new_data));
                        } else {
                            let mut temp_data = current_data_c.borrow_mut();
                            if let Some(temp_data_existing) = temp_data.get(word) {
                                // The data at this location already exists
                                *cursor = ShadowDataCursor::new(Rc::clone(&temp_data_existing), Rc::clone(&cursor.root));
                            } else {
                                // This is the first time this nested object is reached : create data
                                let new_data = ShadowData::wrap(ShadowData::new_object(
                                    Some(selector_id), Weak::clone(&parent)
                                ));
                                temp_data.set(word, Rc::clone(&new_data));
                                *cursor = ShadowDataCursor::new(Rc::clone(&new_data), Rc::clone(&cursor.root));
                            }
                        }
                    } else {
                        if !(is_current && is_current_an_array) {
                            // Assigning intermediate nesting : only when the array is being newly built
                            let mut temp_data = current_data_c.borrow_mut();
                            if let Some(temp_data_existing) = temp_data.get(word) {
                                current_data = Rc::clone(&temp_data_existing);
                            } else {
                                let new_temp_data = ShadowData::wrap(ShadowData::new_object(
                                    Some(selector_id), Weak::clone(&parent)
                                ));
                                temp_data.set(word, Rc::clone(&new_temp_data));
                                current_data = Rc::clone(&new_temp_data);
                            }
                        }
                    }
                }
            }
            let ret = Rc::clone(&cursor.shadow_data);

            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }

    pub fn on_data_tag_close(
        _tag: &mut EndTag,
        _selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        cursor: Rc<RefCell<ShadowDataCursor>>
    ) -> Result<(), ShadowError> {
        if let Some(data_def) = json_def.borrow().data.as_ref() {
            if data_def.path.as_ref().is_some() {
                // A path had been defined : after finishing with this element, go back up
                cursor.borrow_mut().go_up()?;
            }
        }
        Ok(())
    }

    pub fn visualize(&self, tabs: usize) -> String {
        let tab = "  ";
        let tabs_str = tab.repeat(tabs);
        match &self.v {
            ShadowDataValue::String(s) => format!("#{} ^ {} \"{}\"",
                self.uid,
                self.parent.upgrade().unwrap_or_default().borrow().uid,
                s.borrow()
            ),
            ShadowDataValue::Array(a) => {
                format!("#{} ^ {} [\n{}{}\n{}]",
                    self.uid,
                    self.parent.upgrade().unwrap_or_default().borrow().uid,
                    tab.repeat(tabs+1),
                    a.iter().fold(String::new(), |mut acc, s| {
                        if acc.len() > 0 {
                            acc.push_str(&format!(",\n{}", tab.repeat(tabs+1)));
                        }
                        acc.push_str(&s.borrow().visualize(tabs + 1));
                        acc
                    }),
                    tabs_str
                )
            },
            ShadowDataValue::Object(o) => {
                format!("#{} ^ {} {{{}\n{}}}",
                    self.uid,
                    self.parent.upgrade().unwrap_or_default().borrow().uid,
                    o.iter().fold(String::new(), |mut acc, (k,v)| {
                        if acc.len() > 0 {
                            acc.push_str(",");
                        }
                        acc.push_str(&format!("\n{}{}: {}", tab.repeat(tabs+1), k, v.borrow().visualize(tabs + 1)));
                        acc
                    }),
                    tabs_str
                )
            }
        }
    }
}
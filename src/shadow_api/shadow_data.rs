use core::fmt;
use std::{cell::RefCell, rc::{Rc, Weak}, collections::HashMap, any::Any};

use indexmap::IndexMap;
use lol_html::html_content::{Element, EndTag};

use crate::ShadowJson;

use super::{ShadowError};

const CRAWL_CURSOR_CACHE_KEY: &str = "ccc_key";

// ShadowData is a minimalistic tree structure representing json value which contains only Objects, Arrays or Strings, wrapped in Rc<RefCell<T>>
// The reason we don't use serde::json for this is that while serde::json is able to deserialize into Rc (through a feature), RefCells are not supported
#[derive(Debug)]
pub struct ShadowData {
    pub id: Option<usize>,
    pub parent: Weak<RefCell<ShadowData>>,
    pub v: ShadowDataValue
}

#[derive(Debug)]
pub enum ShadowDataValue {
    String(Rc<RefCell<String>>),
    Array(Vec<Rc<RefCell<ShadowData>>>),
    Object(IndexMap<String, Rc<RefCell<ShadowData>>>)
}

impl Default for ShadowData {
    fn default() -> Self {
        ShadowData::new_string(None, Weak::new(), "".to_string())
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
    pub fn new_string(id: Option<usize>, parent: Weak<RefCell<ShadowData>>, v: String) -> Self {
        return ShadowData { id, parent, v: ShadowDataValue::String(Rc::new(RefCell::new(v))) };
    }
    pub fn new_array(id: Option<usize>, parent: Weak<RefCell<ShadowData>>) -> Self {
        return ShadowData { id, parent, v: ShadowDataValue::Array(Vec::new()) };
    }
    pub fn new_object(id: Option<usize>, parent: Weak<RefCell<ShadowData>>) -> Self {
        return ShadowData { id, parent, v: ShadowDataValue::Object(IndexMap::new()) };
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
    // This method parses json_def, and adds to data if necessary
    pub fn prepare_data(
        selector_id: usize,
        data: Rc<RefCell<ShadowData>>, // Previous data
        json_def: &ShadowJson,
        mut parent_array: Weak<RefCell<ShadowData>>,
    ) -> Result<(Rc<RefCell<ShadowData>>, Weak<RefCell<ShadowData>>), ShadowError> { // parent_array might be changed
        let mut next_data: Rc<RefCell<ShadowData>> = data; // Prepare a cell for next loop iteration. Path will nest it
        if let Some(data_def) = json_def.data.as_ref() {
            let path = data_def.path.clone();

            if let Some(mut path) = path {
                // A path is specified => we need to create (or reuse) a deeper element, and overwrite next_data
                let mut is_array = false;
                if path.chars().last().unwrap() == '.' {
                    // Determine whether this element is part of an array of elements
                    is_array = true;
                    path = (path[..path.len() - 1]).to_string(); // Remove the last dot

                    if path.len() == 0 {
                        return Err(ShadowError {
                            msg: "Invalid def : single dot is not accepted, as the definition does not allow a parent to predefine an array".to_string()
                        });
                    }
                }

                let mut split = path.split('.').peekable();
                let mut current_data = Rc::clone(&next_data);
                while let Some(word) = split.next() {
                    let current_data_c = Rc::clone(&current_data);
                    let current_ref = Rc::clone(&current_data);
                    let mut temp_data = current_data_c.borrow_mut();
                    if split.peek().unwrap_or(&"").len() == 0 { // Found last word
                        // Here, we either build a new nested object or fetch an existing one, and assign it to next_data for further processing
                        if is_array {
                            // First fetch an existing array at the given key "word", or create new if none (or if not-array)
                            let data_array = match temp_data.get(word) {
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
                            };
                            parent_array = Rc::downgrade(&data_array); // Creating weak reference to parent array
                            let new_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), parent_array));
                            println!("PUSH AT CREATE");
                            data_array.borrow_mut().push(Rc::clone(&new_data));
                            next_data = Rc::clone(&new_data); // Next data is now pointing to the first (empty) object of the array
                        } else {
                            if let Some(temp_data_existing) = temp_data.get(word) {
                                // The data at this location already exists
                                next_data = Rc::clone(&temp_data_existing);
                            } else {
                                // This is the first time this nested object is reached : create data
                                let new_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), Rc::downgrade(&current_ref)));
                                temp_data.set(word, Rc::clone(&new_data));
                                next_data = Rc::clone(&new_data);
                            }
                            parent_array = Weak::new(); // No parent array => weak reference to nothing
                        }
                    } else {
                        // Assigning intermediate nesting
                        if let Some(temp_data_existing) = temp_data.get(word) {
                            current_data = Rc::clone(&temp_data_existing);
                        } else {
                            let new_temp_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), Rc::downgrade(&current_ref)));
                            temp_data.set(word, Rc::clone(&new_temp_data));
                            current_data = Rc::clone(&new_temp_data);
                        }
                    }
                }
            }
        }

        Ok((next_data, Weak::new()))
    }

    pub fn init_crawl_data(
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) {
        // Only root should have an empty parent
        Self::insert_crawl_cursor(
            cache,
            ShadowData::wrap(ShadowData::new_object(Some(0), Weak::new())
        ));
    }

    pub fn insert_crawl_cursor(
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
        cursor: Rc<RefCell<ShadowData>>
    ) {
        let mut map = cache.borrow_mut();
        map.insert(CRAWL_CURSOR_CACHE_KEY.to_string(), Box::new(cursor));
    }

    // Use this instead of remove_crawl_cursor when cursor is not meant to be modified (contents only)
    pub fn borrow_crawl_cursor_data(
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) -> Result<Rc<RefCell<ShadowData>>, ShadowError> {
        if let Some(data) = cache
            .borrow()
            .get(CRAWL_CURSOR_CACHE_KEY) {
                if let Some(shadow_data_ref) = data.downcast_ref::<Rc<RefCell<ShadowData>>>() {
                    return Ok(Rc::clone(&shadow_data_ref));
                }
            }
        Err(ShadowError { msg: "Crawl cursor not found. Was crawl data properly initialized?".to_string() })
    }

    // Use this instead of get_crawl_cursor_data when updating cursor is needed.
    // Remember to put it back using insert_crawl_cursor afterwards
    pub fn remove_crawl_cursor(
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) -> Result<Rc<RefCell<ShadowData>>, ShadowError> {
        if let Some(mut data) = cache
            .borrow_mut()
            .remove(CRAWL_CURSOR_CACHE_KEY) {
                if let Some(shadowData) = data.downcast_mut::<Rc<RefCell<ShadowData>>>() {
                    return Ok(Rc::clone(&shadowData));
                }
            }
        Err(ShadowError { msg: "Crawl cursor not found. Was crawl data properly initialized?".to_string() })
    }

    // Moves the cursor all the way back up, and returns it removing it from cache
    pub fn take_crawl_cursor_at_top(
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
    ) -> Result<Rc<RefCell<ShadowData>>, ShadowError> {
        let mut cursor = Self::remove_crawl_cursor(Rc::clone(&cache))?;
        loop {
            let parent_weak = Weak::clone(&cursor.borrow_mut().parent);
            if let Some(parent) = parent_weak.upgrade() {
                cursor = parent;
            } else {
                break; // At root
            }
        }
        Ok(cursor)
    }

    // Returns the current Cell 
    pub fn on_data_tag_open(
        el: &mut Element,
        selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) -> Result<Option<Rc<RefCell<ShadowData>>>, ShadowError> {
        if let Some(data_def) = json_def.borrow().data.as_ref() {
            let path = data_def.path.clone();
            let mut cursor = Self::remove_crawl_cursor(Rc::clone(&cache))?;

            let is_current = {
                cursor.borrow_mut().id
                    .and_then(|cur_id| {
                        Some(cur_id == selector_id)
                    })
                    .unwrap_or(false)
            };
            let is_current_an_array = {
                cursor.borrow_mut().is_array()
            };

            if let Some(mut path) = path {
                // A path is specified => we need to create (or reuse) a deeper element, and overwrite next_data
                let mut is_array = false;
                if path.chars().last().unwrap() == '.' {
                    // Determine whether this element is part of an array of elements
                    is_array = true;
                    path = (path[..path.len() - 1]).to_string(); // Remove the last dot

                    if path.len() == 0 {
                        return Err(ShadowError {
                            msg: "Invalid def : single dot is not accepted, as the definition does not allow a parent to predefine an array".to_string()
                        });
                    }
                }

                let mut split = path.split('.').peekable();
                let mut current_data = Rc::clone(&cursor);
                while let Some(word) = split.next() {
                    let current_data_c = Rc::clone(&current_data);
                    let current_ref = Rc::clone(&current_data);
                    let mut temp_data = current_data_c.borrow_mut();
                    if split.peek().unwrap_or(&"").len() == 0 { // Found last word
                        // Here, we either build a new nested object or fetch an existing one, and assign it to next_data for further processing
                        if is_array {
                            // First fetch an existing array at the given key "word", or create new if none (or if not-array)
                            let data_array = match temp_data.get(word) {
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
                            };

                            if is_current && is_current_an_array {
                                // Case of coming back to non-first detected element for this selector => push new item and repoint cursor
                                let new_item = ShadowData::wrap(ShadowData::new_object(
                                    Some(selector_id), // Array and its elements both share the selector id
                                    Rc::downgrade(&cursor)
                                ));
                                {
                                //    cursor.borrow_mut().as_array_mut().unwrap().push(Rc::clone(&new_item));
                                }
                                //cursor = new_item;
                                println!("ARRAY SIBLING !");
                            }
                            let parent_array = Rc::downgrade(&data_array); // Creating weak reference to parent array
                            let new_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), parent_array));
                            data_array.borrow_mut().push(Rc::clone(&new_data));
                            cursor = Rc::clone(&new_data); // Next data is now pointing to the first (empty) object of the array
                        } else {
                            if let Some(temp_data_existing) = temp_data.get(word) {
                                // The data at this location already exists
                                cursor = Rc::clone(&temp_data_existing);
                            } else {
                                // This is the first time this nested object is reached : create data
                                let new_data = ShadowData::wrap(ShadowData::new_object(
                                    Some(selector_id), Rc::downgrade(&current_ref)
                                ));
                                temp_data.set(word, Rc::clone(&new_data));
                                cursor = Rc::clone(&new_data);
                            }
                        }
                    } else {
                        // Assigning intermediate nesting
                        if let Some(temp_data_existing) = temp_data.get(word) {
                            current_data = Rc::clone(&temp_data_existing);
                        } else {
                            let new_temp_data = ShadowData::wrap(ShadowData::new_object(Some(selector_id), Rc::downgrade(&current_ref)));
                            temp_data.set(word, Rc::clone(&new_temp_data));
                            current_data = Rc::clone(&new_temp_data);
                        }
                    }
                }
            }

            let ret = Rc::clone(&cursor);
            Self::insert_crawl_cursor(cache, cursor); // Put the cursor back

            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }

    pub fn on_data_tag_close(
        tag: &mut EndTag,
        selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) -> Result<(), ShadowError> {
        if let Some(data_def) = json_def.borrow().data.as_ref() {
            if data_def.path.as_ref().is_some() {
                // If a path is defined, then a new nested element must had been added => go up the tree once
                let mut cursor = Self::remove_crawl_cursor(Rc::clone(&cache))?;
                let parent_weak = Weak::clone(&cursor.borrow_mut().parent);
                if let Some(parent) = parent_weak.upgrade() {
                    cursor = parent;
                } else {
                    return Err(ShadowError {
                        msg: format!("on_data_tag_close[{}|{}]: No parent. Cannot move up the tree", tag.name(), selector_id)
                    });
                }
                Self::insert_crawl_cursor(cache, cursor); // Put the cursor back
            }
        }
        Ok(())
    }
}
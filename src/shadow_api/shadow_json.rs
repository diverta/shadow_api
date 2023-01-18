use std::cell::{RefCell};
use std::rc::Rc;
use std::str;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "source", content = "name")]
// We use adjacently tagged representation. Refer to https://serde.rs/enum-representations.html
pub enum ShadowJsonValueSource {
    Contents, // Current node's contents will be used (as string)
    Attribute(String), // Current node's specified attribute will be used
    Value, // Current node's value will be used. This is useful with various form elements such as Select, Input etc. An error will be pushed if current node does not implement support for Value
}
#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ShadowJsonData {
    /*
        Target json key path, where values extracted from this node will be stored.
        If the last character is a dot '.', parent will be considered as array and new element will be appended to it.
        Otherwise, new elements will be inserted as keys of an object
        If multiple keys are separated with dots, nested objects will be generated.
        Path can be omitted in children (after being specified at least once), in which case the parent's current path will be used.
        Examples :
            "first.second" => target json : {"first": {"second": { ...(parsed values as keys) }}}
            "first.second." => target json : {"first": {"second": [ (parsed values as separate elements of array) ]}}
    */
    pub path: Option<String>,
    /*
        A map where key represents the name of the value, and value indicates how the data should be extracted from the current node
    */
    pub values: Option<IndexMap<String, ShadowJsonValueSource>>
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ShadowJson {
    pub s: String, // selector of an element
    pub hide: Option<bool>, // Whether this element should be hidden or not
    pub delete: Option<bool>, // Whether this element should be deleted or not

    pub edit: Option<ShadowJsonEdit>,

    //  Indicates how to extract the data out of the current node. Applies AFTER attribute/content edit
    pub data: Option<ShadowJsonData>,

    // HTML injection operators
    pub append: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Appends a new child, after existing children
    pub prepend: Option<Vec<String>>,  // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Appends a new child, before existing children
    pub insert_before: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Inserts a new sibling before this node
    pub insert_after: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Inserts a new sibling after this node

    // Recursive structure
    pub sub: Option<Rc<Vec<Rc<RefCell<ShadowJson>>>>>, // For subselectors having the same struct
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ShadowJsonEdit {
    pub attrs: Option<IndexMap<String, ShadowJsonEditOne>>,
    pub content: Option<ShadowJsonEditOne>,
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ShadowJsonEditOne {
    pub op: String,
    pub val: Option<String>
}
impl ShadowJson {
    // Wrapper function to unformize deserialization and add global error handling
    pub fn parse_str(json: &str, errors: Rc<RefCell<Vec<String>>>) -> Self {
        // New lines are not allowed in json multi-line string values => just remove all of them
        let json_processed = json.replace("\n", "").replace("  ", " ");
        let jd = &mut serde_json::Deserializer::from_str(json_processed.as_str());
        let result: Result<ShadowJson, _> = serde_path_to_error::deserialize(jd);

        match result {
            Ok(parsed) => parsed,
            Err(err) => {
                let mut errors_m = errors.borrow_mut();
                errors_m.push(format!("Invalid json : {}", err.to_string()));
                ShadowJson::default()
            }
        }
    }

    // Useful for replacing values in parsed ShadowJson
    pub fn transform_strings(&mut self, f: fn(&mut String)) {
        f(&mut self.s);

        if let Some(edit) = &mut self.edit {
            if let Some(attrs) = &mut edit.attrs {
                attrs.iter_mut().for_each(|attr| {
                    if let Some(val) = &mut attr.1.val {
                        f(val);
                    }
                });
            }
            if let Some(content) = &mut edit.content {
                if let Some(val) = &mut content.val {
                    f(val);
                }
            }
        }

        if let Some(append) = &mut self.append {
            append.iter_mut().for_each(|a| {
                f(a);
            });
        }
        if let Some(prepend) = &mut self.prepend {
            prepend.iter_mut().for_each(|a| {
                f(a)
            });
        }
        if let Some(insert_before) = &mut self.insert_before {
            insert_before.iter_mut().for_each(|a| {
                f(a)
            });
        }
        if let Some(insert_after) = &mut self.insert_after {
            insert_after.iter_mut().for_each(|a| {
                f(a)
            });
        }

        // Recursive replacement
        if let Some(sub) = &self.sub {
            sub.iter().for_each(|el| {
                el.borrow_mut().transform_strings(f);
            })
        }
    }
}
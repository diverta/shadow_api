use std::cell::{RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::str;
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
    pub values: Option<HashMap<String, ShadowJsonValueSource>>
}

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ShadowJson {
    pub s: String, // selector of an element
    pub hide: Option<bool>, // Whether this element should be hidden or not
    pub delete: Option<bool>, // Whether this element should be deleted or not
    pub sub: Option<Rc<Vec<Rc<ShadowJson>>>>, // For subselectors having the same struct
    pub data: Option<ShadowJsonData>, // Indicates how to extract the data out of the current node
    // HTML injection operators
    pub append: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Appends a new child, after existing children
    pub prepend: Option<Vec<String>>,  // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Appends a new child, before existing children
    pub insert_before: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Inserts a new sibling before this node
    pub insert_after: Option<Vec<String>>, // Vec of HTML tags. If an item defines multiple tags, only the first one will be parsed. Inserts a new sibling after this node
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
}
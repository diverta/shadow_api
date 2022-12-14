//! ShadowAPI is a streaming html processor that is able to :
//! - Modify DOM on the fly (inject/edit/delete tags and attributes)
//! - Read DOM parts (element attributes and content) according to a specification defined by ShadowJson and inject the collected information as a json variable
//! 
//! The 4 steps are :
//! 1. Construct a ShadowJson object which should contain the definition of DOM modifications and data collection.
//! 2. Optionnally define how the data should be injected. By default, ShadowApi creates a JS variable `shadow_api_data` enclosed in <script> tags. This is always inserted right before the </body> closing tag
//! 3. Call the ShadowApi::parse method on the constructed object. You will also need to provide `errors` object to store potential errors in your data definition for debugging.
//! 4. Finally, call `ShadowApi::stream_response` to begin processing Fastly response chunk by chunk
//! 
//! It is recommended that the steps 1,2 and 3 are done while waiting for the backend response (using `Fastly::Request::send_async` for example) - especially if ShadowJson is fetched through another API.

use std::cell::{RefCell};
use std::io::{Write};
use std::rc::{Rc, Weak};
use std::borrow::{Cow};
use indexmap::IndexMap;
use lol_html::html_content::{ContentType, Element, TextChunk};
use lol_html::{ElementContentHandlers, Selector, HtmlRewriter, Settings};

mod shadow_data;
mod shadow_json;

pub use shadow_data::ShadowData;
pub use shadow_json::ShadowJson;
use shadow_json::ShadowJsonValueSource;

pub struct ShadowApi<'a> {
    pub data: Rc<RefCell<ShadowData>>,
    data_formatter: Rc<Box<dyn Fn(String) -> String>>,
    ech: RefCell<Vec<(Cow<'a, Selector>, ElementContentHandlers<'a>)>>
}

impl ShadowApi<'_> {
    pub fn new() -> Self {
        ShadowApi {
            data: ShadowData::wrap(ShadowData::new_object()),
            data_formatter: Rc::new(Box::new(Self::default_data_formatter)),
            ech: RefCell::new(Vec::new())
        }
    }

    pub fn set_data_formatter(&mut self, formatter: Rc<Box<dyn Fn(String) -> String>>) {
        self.data_formatter = formatter;
    }

    /// Override this method to customize how you inject data.
    /// The contents will always be inserted at right before the end of </body> tag, as the data is collected while the body is streamed
    fn default_data_formatter(data: String) -> String {
        return format!(r##"<script>var shadow_api_data = {};</script>"##, data);
    }

    // Parses a ShadowJson into a Vec destined for building ElementContentHandlers of LOLHTML Crate
    // json_def: Vec of ShadowJson
    // errors : A container to write errors to
    pub fn parse(
        &self,
        json_def: Rc<Vec<Rc<ShadowJson>>>,
        errors: Rc<RefCell<Vec<String>>>
    ) {
        let mut selector_stack: Vec<String> = Vec::with_capacity(10);
        let mut ech_borrowed = self.ech.borrow_mut();
        let ech = ech_borrowed.as_mut();
        Self::parse_rec(
            json_def,
            errors,
            Rc::clone(&self.data),
            Weak::new(),
            ech,
            &mut selector_stack
        );
        Self::data_content_handler(Rc::clone(&self.data), Rc::clone(&self.data_formatter), ech); // This will create a special handler to inject data at the end
    }

    fn parse_rec(
        json_def: Rc<Vec<Rc<ShadowJson>>>,
        errors: Rc<RefCell<Vec<String>>>,
        data: Rc<RefCell<ShadowData>>,
        parent_array: Weak<RefCell<ShadowData>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>,
        selector_stack: &mut Vec<String> // To build full selector
    ) {
        for el in json_def.as_ref() {
            Self::parse_one(
                Rc::clone(&el),
                Rc::clone(&errors),
                Rc::clone(&data),
                Weak::clone(&parent_array),
                ech,
                selector_stack
            );
        }
    }

    fn parse_one(
        json_def: Rc<ShadowJson>,
        errors_rc: Rc<RefCell<Vec<String>>>,
        data: Rc<RefCell<ShadowData>>,
        mut parent_array: Weak<RefCell<ShadowData>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>,
        selector_stack: &mut Vec<String> // To build full selector
    ) {
        if json_def.s.as_str().len() == 0 {
            let mut errors = errors_rc.borrow_mut();
            errors.push("Empty selector".to_string());
            return;
        }
        selector_stack.push(json_def.s.clone());
        let current_selector = selector_stack.join(" "); // Since LOLHTML is not building dom tree, we need to build the absolute selector

        let mut next_data: Rc<RefCell<ShadowData>> = Rc::clone(&data); // Prepare a cell for next loop iteration. Path will nest it
        let path: Option<String>;
        if let Some(data_def) = &json_def.data {
            path = data_def.path.clone();

            if let Some(mut path) = path {
                // A path is specified => we need to create (or reuse) a deeper element, and overwrite next_data
                let mut is_array = false;
                if path.chars().last().unwrap() == '.' {
                    // Determine whether this element is part of an array of elements
                    is_array = true;
                    path = (path[..path.len() - 1]).to_string(); // Remove the last dot

                    if path.len() == 0 {
                        let mut errors = errors_rc.borrow_mut();
                        errors.push("Invalid def : single dot is not accepted, as the definition does not allow a parent to predefine an array".to_string());
                        return;
                    }
                }

                let mut split = path.split('.').peekable();
                let mut current_data = data.clone();
                while let Some(word) = split.next() {
                    let current_data_c = current_data.clone();
                    let mut temp_data = current_data_c.borrow_mut();
                    if split.peek().unwrap_or(&"").len() == 0 { // Found last word
                        // Here, we either build a new nested object or fetch an existing one, and assign it to next_data for further processing
                        if is_array {
                            // First fetch an existing array at the given key "word", or create new if none (or if not-array)
                            let data_array = match temp_data.get(word) {
                                Some(existing_el) => {
                                    let existing_el_rc = Rc::clone(&existing_el);
                                    let array_el = match &*existing_el_rc.borrow() {
                                        ShadowData::String(_) | ShadowData::Object(_) => {
                                            let new_array = ShadowData::wrap(ShadowData::new_array());
                                            temp_data.set(word, Rc::clone(&new_array));
                                            new_array
                                        },
                                        ShadowData::Array(_) => existing_el
                                    };
                                    array_el
                                },
                                None => {
                                    let array_el = ShadowData::wrap(ShadowData::new_array());
                                    temp_data.set(word, Rc::clone(&array_el));
                                    array_el
                                }
                            };
                            parent_array = Rc::downgrade(&data_array); // Creating weak reference to parent array
                            let new_data = ShadowData::wrap(ShadowData::new_object());
                            data_array.borrow_mut().push(Rc::clone(&new_data));
                            next_data = Rc::clone(&new_data); // Next data is now pointing to the first (empty) object of the array
                            break;
                        } else {
                            if let Some(temp_data_existing) = temp_data.get(word) {
                                // The data at this location already exists
                                next_data = Rc::clone(&temp_data_existing);
                            } else {
                                // This is the first time this nested object is reached : create data
                                let new_data = ShadowData::wrap(ShadowData::new_object());
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
                            let new_temp_data = ShadowData::wrap(ShadowData::new_object());
                            temp_data.set(word, Rc::clone(&new_temp_data));
                            current_data = Rc::clone(&new_temp_data);
                        }
                    }
                }
            }
        }

        // Element handler function: it processes the node as an element
        let mut use_element_handler = false;
        let mut use_text_handler = false;
        if json_def.delete.unwrap_or(false) {
            use_element_handler = true;
        }
        let empty_vec = Vec::new();
        if json_def.hide.unwrap_or(false)
            || json_def.insert_after.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def.insert_before.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def.append.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def.prepend.as_ref().unwrap_or(&empty_vec).len() > 0
            {
            use_element_handler = true;
        }
        if let Some(_html_tags) = &json_def.insert_after {
            use_element_handler = true;
        }
        if let Some(_html_tags) = &json_def.insert_before {
            use_element_handler = true;
        }
        if let Some(data_def) = &json_def.data {
            if let Some(values) = &data_def.values {
                if !values.is_empty() {
                    for (_key, value) in values.iter() {
                        match value {
                            ShadowJsonValueSource::Attribute(_attr_name) => {
                                use_element_handler = true;
                            },
                            ShadowJsonValueSource::Contents => {
                                use_text_handler = true;
                            },
                            ShadowJsonValueSource::Value => {
                                use_element_handler = true;
                            }
                        }
                    }
                } else {
                    let mut errors = errors_rc.borrow_mut();
                    errors.push("Invalid def : 'data.values' is not an object".to_string());
                    use_element_handler = false;
                    use_text_handler = false;
                }
            }
        }

        if use_element_handler {
            let eh_errors = Rc::clone(&errors_rc);
            let eh_json_def = Rc::clone(&json_def);
            let eh_data = Rc::clone(&next_data);
            let parent_array_cloned = Weak::clone(&parent_array);
            ech.push((
                Cow::Owned(current_selector.parse().unwrap()),
                ElementContentHandlers::default().element(move |el| {
                    Self::element_content_handler(
                        el,
                        Rc::clone(&eh_json_def),
                        Rc::clone(&eh_data),
                        Weak::clone(&parent_array_cloned),
                        Rc::clone(&eh_errors)
                    )
                })
            ));
        }
        if use_text_handler {
            let th_errors = Rc::clone(&errors_rc);
            let th_json_def = Rc::clone(&json_def);
            let th_data = Rc::clone(&next_data);
            let parent_array_cloned = Weak::clone(&parent_array);
            ech.push((
                Cow::Owned(current_selector.parse().unwrap()),
                ElementContentHandlers::default().text(move |el| {
                    Self::text_content_handler(
                        el,
                        Rc::clone(&th_json_def),
                        Rc::clone(&th_data),
                        Weak::clone(&parent_array_cloned),
                        Rc::clone(&th_errors)
                    )
                })
            ));
        }

        if let Some(sub) = &json_def.sub {
            ShadowApi::parse_rec(
                Rc::clone(&sub),
                Rc::clone(&errors_rc),
                Rc::clone(&next_data),
                parent_array,
                ech,
                selector_stack
            );
        }

        selector_stack.pop();
    }

    fn element_content_handler(
        el: &mut Element,
        json_def: Rc<ShadowJson>,
        mut new_data_init: Rc<RefCell<ShadowData>>,
        parent_array: Weak<RefCell<ShadowData>>,
        errors: Rc<RefCell<Vec<String>>>
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if json_def.hide.unwrap_or(false) {
            match el.get_attribute("style") {
                Some(mut style) => style.push_str(";display: none"),
                None => el.set_attribute("style", "display: none").unwrap_or_else(|_| {}),
            }
        }
        if let Some(html_tags) = &json_def.insert_after {
            for tag in html_tags {
                el.after(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def.insert_before {
            for tag in html_tags {
                el.before(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def.append {
            for tag in html_tags {
                el.append(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def.prepend {
            for tag in html_tags {
                el.prepend(tag.as_str(), ContentType::Html)
            }
        }
        
        if let Some(data_def) = &json_def.data {
            if let Some(values) = &data_def.values {
                if !values.is_empty() {
                    let attrs = el
                        .attributes()
                        .iter()
                        .map(|a| (a.name(), a.value()))
                        .collect::<IndexMap<String, String>>();
                    for (key, value) in values.iter() {
                        match value {
                            ShadowJsonValueSource::Attribute(attr_name) => {
                                if attr_name.len() == 0 { continue; }
                                Self::prepare_array_element(Rc::clone(&new_data_init), Weak::clone(&parent_array), key);
                                if let Some(attr_value) = attrs.get(attr_name) {
                                    let mut new_data_m = new_data_init.borrow_mut();
                                    new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(attr_value.clone())));
                                }
                            },
                            ShadowJsonValueSource::Contents => {
                                // This is handled by text_content_handler
                            },
                            ShadowJsonValueSource::Value => {
                                Self::prepare_array_element(Rc::clone(&new_data_init), Weak::clone(&parent_array), key);
                                // Fetch the current value from the different form elements
                                match el.tag_name().as_str() {
                                    /* LOLHTML does not allow to operate on children, so to provide "select" shortcut we would need to create a new handler its children
                                    * However whether the element is select or not is unknown before parsing the element itself, and it is too late to add
                                    * a new handler at that point. So we cannot provide "select" shortcut. Instead use directly "select > option[selected=selected]"
                                    "select" => {},
                                    */
                                    "input" => {
                                        if let Some(input_type) = attrs.get("type") {
                                            let mut new_data_m = new_data_init.borrow_mut();
                                            match input_type.as_str() {
                                                "radio" => {
                                                    if attrs.get("checked").is_some() {
                                                        // For radio/checkbox, we only consider the box which is checked. Make sure def json contains all items
                                                        new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(attrs.get("value").unwrap_or(&String::from("")).to_owned())));
                                                    } else if new_data_m.get(key).is_none() {
                                                        // Init
                                                        new_data_m.set(key, ShadowData::wrap(ShadowData::new_string("".to_string())));
                                                    }
                                                }
                                                "checkbox" => {
                                                    if new_data_m.get(key).is_none() {
                                                        new_data_m.set(key, ShadowData::wrap(ShadowData::new_array()));
                                                    }
                                                    if attrs.get("checked").is_some() {
                                                        // For radio/checkbox, we only consider the box which is checked. Make sure def json contains all items
                                                        if let Some(arr) = new_data_m.get(key) {
                                                            let mut arr_borrowed = arr.borrow_mut();
                                                            arr_borrowed.push(ShadowData::wrap(ShadowData::new_string(attrs.get("value").unwrap_or(&String::from("")).to_owned())));
                                                        }
                                                    }
                                                }
                                                _ => {
                                                    new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(attrs.get("value").unwrap_or(&String::from("").to_string()).to_owned())));
                                                }
                                            }
                                        }
                                    },
                                    "option" => {
                                        let mut new_data_m = new_data_init.borrow_mut();
                                        new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(attrs.get("value").unwrap_or(&String::from("").to_string()).to_owned())));
                                    },
                                    _ => {
                                        let mut errors_m = errors.borrow_mut();
                                        errors_m.push(format!("Unimplemented input: '{}' (TODO)",el.tag_name().as_str()));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let mut errors_m = errors.borrow_mut();
                    errors_m.push("Invalid def : 'data.values' is not an object".to_string());
                    return Ok(());
                }
            }
        }
        if json_def.delete.unwrap_or(false) {
            el.remove();
        }

        Ok(())
    }

    fn prepare_array_element(
        current_el: Rc<RefCell<ShadowData>>,
        parent_array: Weak<RefCell<ShadowData>>,
        key: &String
    ) {
        if parent_array.strong_count() > 0 {
            // The parent array exists, meaning that new_data_init is an element of the array.
            // We need to decide if we should modify the current element, or to append a new one (and repoint new_data_init to it)
            // This decision will be based on the existence of a value with the same key - if yet, it is *most likely* a new selector match
            let create_new_el: bool = {
                let new_data_m = current_el.borrow_mut();
                if let Some(new_data_obj) = new_data_m.as_object() {
                    // This should always be the case
                    new_data_obj.contains_key(key)
                } else {
                    false
                }
            };
            if create_new_el {
                if let Some(parent) = parent_array.upgrade() {
                    // Since strong_count was not zero, upgrade should always yield Some
                    *current_el.borrow_mut() = ShadowData::new_object();
                    parent.borrow_mut().push(Rc::clone(&current_el));
                }
            }
        }
    }

    fn text_content_handler(
        el: &mut TextChunk,
        json_def: Rc<ShadowJson>,
        new_data_init: Rc<RefCell<ShadowData>>,
        parent_array: Weak<RefCell<ShadowData>>,
        _errors: Rc<RefCell<Vec<String>>>
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        static mut ALL_REPLACE_TEXT_BUFFER: String = String::new();
        unsafe { // Mutable static => unsafe. But in our case, it is safe because we don't do multithreading
            ALL_REPLACE_TEXT_BUFFER.push_str(el.as_str()); // Saved chunk to buffer
        }
        el.remove();
        if el.last_in_text_node() {
            // Last text chunk reached : process the buffer, send it back and reset it
            // PROCESSING BEGINS
            if let Some(data_def) = &json_def.data {
                if let Some(values) = &data_def.values {
                    if !values.is_empty() {
                        for (key, value) in values.iter() {
                            match value {
                                ShadowJsonValueSource::Contents => {
                                        Self::prepare_array_element(Rc::clone(&new_data_init), Weak::clone(&parent_array), key);
                                        let mut new_data_m = new_data_init.borrow_mut();
                                        unsafe { // Mutable static => unsafe in multi-threaded env. But in our case, it is safe because we don't do multithreading
                                            new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(ALL_REPLACE_TEXT_BUFFER.clone())));
                                        }
                                },
                                _ => {
                                    // Handled by element_content_handler
                                }
                            }
                        }
                    }
                }
            }
            // PROCESSING ENDS
            unsafe { // Mutable static => unsafe. But in our case, it is safe because we don't do multithreading
                el.replace(&ALL_REPLACE_TEXT_BUFFER, ContentType::Text);
                ALL_REPLACE_TEXT_BUFFER.clear(); // Reset
            }
        }
        Ok(())
    }

    fn data_content_handler(
        data: Rc<RefCell<ShadowData>>,
        data_formatter: Rc<Box<dyn Fn(String) -> String>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>
    ) {
        ech.push((
            Cow::Owned("body".parse().unwrap()),
            ElementContentHandlers::default().element(move |el| {
                let data = Rc::clone(&data);
                let data_formatter_c = Rc::clone(&data_formatter);
                el.on_end_tag(move |end| {
                    let data_b = data.borrow_mut();
                    let props_html: String = (data_formatter_c)(data_b.to_string());
                    end.before(props_html.as_str(), ContentType::Html);
                    Ok(())
                })?;
                Ok(())
            })
        ));
    }

    pub fn process_html<W, R>(&self, writer: &mut W, chunk_iter: &mut R, errors: Rc<RefCell<Vec<String>>>)
    where
        W: Write,
        R: Iterator<Item = Result<Vec<u8>, std::io::Error>>
    {
        let mut errors_rewrite_client: Vec<String> = Vec::new();
        let ech = self.ech.take(); // This is the last time we use ech, so we can remove it
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: ech,
                ..Settings::default()
            },
            |c: &[u8]| {
                if let Err(e) = writer.write(c) {
                    errors_rewrite_client.push(format!("Error writing to client body : {}",e));
                }
            }
        );

        for chunk in chunk_iter {
            if let Ok(chunk_data) = chunk {
                if let Err(e) = rewriter.write(&chunk_data) {
                    let mut errors_m = errors.borrow_mut();
                    errors_m.push(format!("Error writing to rewriter : {}", e));
                }
            }
        }
        {
            let mut errors_m = errors.borrow_mut();
            errors_m.append(&mut errors_rewrite_client);
        }
    }
}
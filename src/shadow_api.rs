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

use std::any::Any;
use std::cell::{RefCell};
use std::collections::HashMap;
use std::io::{Write, Read};
use std::rc::{Rc, Weak};
use std::borrow::{Cow};
use std::str::FromStr;
use std::sync::atomic::AtomicUsize;
use indexmap::IndexMap;
use lol_html::html_content::{ContentType, Element, TextChunk};
use lol_html::{ElementContentHandlers, Selector, HtmlRewriter, Settings, OutputSink};

mod shadow_error;
mod shadow_data;
mod shadow_data_cursor;
mod shadow_json;

use regex::Regex;
use serde::{Deserialize, Serialize};
pub use shadow_error::ShadowError;
pub use shadow_data::ShadowData;
pub use shadow_json::ShadowJson;
pub use shadow_data_cursor::ShadowDataCursor;
use shadow_json::ShadowJsonValueSource;

const MAX_CHUNK_BYTESIZE: usize = 8096;

pub struct ShadowApi<'a> {
    data_formatter: Rc<Box<dyn Fn(String) -> String>>,
    pub ech: RefCell<Vec<(Cow<'a, Selector>, ElementContentHandlers<'a>)>>,
    max_chunk_bytesize: usize,
    options: Option<ShadowApiOptions>,
    pub shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>,
}

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone)]
pub struct ShadowApiOptions {
    #[serde(default)]
    pub as_json: bool,
}

impl<'h> ShadowApi<'h> {
    pub fn new(options: Option<ShadowApiOptions>) -> Self {
        ShadowApi {
            data_formatter: Rc::new(Box::new(Self::default_data_formatter)),
            ech: RefCell::new(Vec::new()),
            max_chunk_bytesize: MAX_CHUNK_BYTESIZE,
            options,
            shadow_data_cursor: Rc::new(RefCell::new(ShadowDataCursor::init()))
        }
    }

    pub fn set_max_chunk_bytesize(&mut self, bytesize: usize) {
        self.max_chunk_bytesize = bytesize;
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
    // Returns the cache
    pub fn parse(
        &self,
        json_def: Rc<Vec<Rc<RefCell<ShadowJson>>>>,
        errors: Rc<RefCell<Vec<String>>>
    ) -> Rc<RefCell<HashMap<String, Box<dyn Any>>>> {
        let mut selector_stack: Vec<String> = Vec::with_capacity(10);
        let mut ech_borrowed = self.ech.borrow_mut();
        let ech = ech_borrowed.as_mut();
        let cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>> = Rc::new(RefCell::new(HashMap::new()));
        {
            let mut cache_borrowed = cache.borrow_mut();

            // Cache for computed regex executed while stream processing the HTML
            let regex_map: HashMap<String, Regex> = HashMap::new();
            cache_borrowed.insert(String::from("regex_map"), Box::new(regex_map));
        }
        Self::parse_rec(
            json_def,
            errors,
            ech,
            &mut selector_stack,
            Rc::clone(&cache),
            Rc::clone(&self.shadow_data_cursor),
        );
        let dom_written = self.options.as_ref().and_then(|opt| Some(!opt.as_json)).unwrap_or(true);
        if dom_written {
            // No need for data content DOM injection if "as_json" option is set
            Self::data_content_handler(
                Rc::clone(&self.data_formatter),
                ech,
                Rc::clone(&self.shadow_data_cursor)
            ); // This will create a special handler to inject data at the end
        }
        cache
    }

    fn parse_rec(
        json_def: Rc<Vec<Rc<RefCell<ShadowJson>>>>,
        errors: Rc<RefCell<Vec<String>>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>,
        selector_stack: &mut Vec<String>, // To build full selector
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
        shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>
    ) {
        for el in json_def.as_ref() {
            Self::parse_one(
                Rc::clone(&el),
                Rc::clone(&errors),
                ech,
                selector_stack,
                Rc::clone(&cache),
                Rc::clone(&shadow_data_cursor)
            );
        }
    }

    fn parse_one(
        json_def: Rc<RefCell<ShadowJson>>,
        errors_rc: Rc<RefCell<Vec<String>>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>,
        selector_stack: &mut Vec<String>, // To build full selector
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
        shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>
    ) {
        static COUNTER: AtomicUsize = AtomicUsize::new(1);
        let selector_id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let json_def_b = json_def.borrow();
        if json_def_b.s.as_str().len() == 0 {
            let mut errors = errors_rc.borrow_mut();
            errors.push("Empty selector".to_string());
            return;
        }
        selector_stack.push(json_def_b.s.clone());
        let current_selector = selector_stack.join(" "); // Since LOLHTML is not building dom tree, we need to build the absolute selector

        // Validating the selector
        let current_selector_obj = match Selector::from_str(&current_selector) {
            Ok(s) => s,
            Err(e) => {
                errors_rc.borrow_mut().push(format!("Selector {} is invalid : {}", &current_selector, e));
                return;
            },
        };

        /* No need to prepare the data before the crawl, as it is dynamic
        let (next_data, parent_array) = match ShadowData::prepare_data(
            selector_id,
            Rc::clone(&data),
            &json_def_b,
            Weak::new(),
        ) {
            Ok(data) => data,
            Err(err) => {
                errors_rc.borrow_mut().push(err.to_string());
                return;
            },
        };
        */

        // Element handler function: it processes the node as an element
        let mut use_element_handler = false;
        let mut use_text_handler = false;
        let empty_vec = Vec::new();

        if // Listing all cases where we will need to generate an ECH for the element. Minimizing the cases will improve runtime performance
            json_def_b.hide.unwrap_or(false)
            || json_def_b.insert_after.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def_b.insert_before.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def_b.append.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def_b.prepend.as_ref().unwrap_or(&empty_vec).len() > 0
            || json_def_b.edit.is_some()
            || json_def_b.delete.unwrap_or(false)
            || json_def_b.data.as_ref()
                .and_then(|sd| {
                    Some(sd.path.as_ref().unwrap_or(&"".to_owned()).len() > 0)
                })
                .unwrap_or(false)
        {
            use_element_handler = true;
        }
        if let Some(data_def) = &json_def_b.data {
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
        if let Some(edit) = &json_def_b.edit {
            if edit.content.is_some() {
                use_text_handler = true;
            }
        }

        if use_element_handler {
            // Getting an extra RC before moving these into closure
            let eh_errors = Rc::clone(&errors_rc);
            let eh_json_def = Rc::clone(&json_def);
            let eh_cache = Rc::clone(&cache);
            let eh_shadow_data_cursor = Rc::clone(&shadow_data_cursor);

            ech.push((
                Cow::Owned(current_selector_obj.clone()),
                ElementContentHandlers::default().element(move |el| {
                    Self::element_content_handler(
                        el,
                        selector_id,
                        Rc::clone(&eh_json_def),
                        Rc::clone(&eh_errors),
                        Rc::clone(&eh_cache),
                        Rc::clone(&eh_shadow_data_cursor)
                    )
                })
            ));
        }
        if use_text_handler {
            // Getting an extra RC before moving these into closure
            let th_errors = Rc::clone(&errors_rc);
            let th_json_def = Rc::clone(&json_def);
            let th_cache = Rc::clone(&cache);
            let th_content_buffer = Rc::new(RefCell::new(String::new())); // Text content buffer is local for each selector
            let th_shadow_data_cursor = Rc::clone(&shadow_data_cursor);

            ech.push((
                Cow::Owned(current_selector_obj),
                ElementContentHandlers::default().text(move |el| {
                    Self::text_content_handler(
                        el,
                        selector_id,
                        Rc::clone(&th_json_def),
                        Rc::clone(&th_errors),
                        Rc::clone(&th_content_buffer),
                        Rc::clone(&th_cache),
                        Rc::clone(&th_shadow_data_cursor)
                    )
                })
            ));
        }

        if let Some(sub) = &json_def_b.sub {
            ShadowApi::parse_rec(
                Rc::clone(&sub),
                Rc::clone(&errors_rc),
                ech,
                selector_stack,
                Rc::clone(&cache),
                Rc::clone(&shadow_data_cursor)
            );
        }

        selector_stack.pop();
    }

    fn element_content_handler(
        el: &mut Element,
        selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        errors: Rc<RefCell<Vec<String>>>,
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
        shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json_def_c = Rc::clone(&json_def);
        let json_def_b = json_def.borrow();
        let delete = json_def_b.delete.unwrap_or(false);

        if let Some(html_tags) = &json_def_b.insert_after {
            for tag in html_tags {
                el.after(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def_b.insert_before {
            for tag in html_tags {
                el.before(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def_b.append {
            for tag in html_tags {
                el.append(tag.as_str(), ContentType::Html)
            }
        }
        if let Some(html_tags) = &json_def_b.prepend {
            for tag in html_tags {
                el.prepend(tag.as_str(), ContentType::Html)
            }
        }

        if !delete {
            if json_def_b.hide.unwrap_or(false) {
                match el.get_attribute("style") {
                    Some(mut style) => style.push_str(";display: none"),
                    None => el.set_attribute("style", "display: none").unwrap_or_else(|_| {}),
                }
            }
            if let Some(edit) = &json_def_b.edit {
                if let Some(attrs) = &edit.attrs {
                    for (key, val) in attrs.iter() {
                        match val.op.as_str() {
                            "delete" => {
                                el.remove_attribute(key);
                            }
                            "upsert" => {
                                if let Some(value) = &val.val {
                                    if let Err(e) = el.set_attribute(key, value.as_str()) {
                                        errors.borrow_mut().push(format!("Unable to set attribute (edit.attrs.{}): {}", key, e));
                                    }
                                } else {
                                    errors.borrow_mut().push(format!("Upsert requires val attribute (edit.attrs.{})", key));
                                }
                            }
                            "match_replace" => {
                                if let Some(r#match) = &val.r#match {
                                    if let Some(new_value) = &val.val {
                                        let old_value = &el.get_attribute(key).unwrap_or("".to_owned());
                                        if let Some(replacement) = Self::match_replace(
                                            r#match,
                                            old_value,
                                            new_value,
                                            Rc::clone(&errors),
                                            Rc::clone(&cache)
                                        ) {
                                            if let Err(e) = el.set_attribute(key, &replacement) {
                                                errors.borrow_mut().push(format!("Unable to set attribute via match_replace (edit.attrs.{}): {}", key, e));
                                            }
                                        }
                                    }
                                }
                            }
                            other => {
                                errors.borrow_mut().push(format!("Invalid operation (edit.attrs.{}): {}. Allowed values : delete/upsert/match_replace", key, other));
                            }
                        }
                    }
                }
            }
        }

        match ShadowData::on_data_tag_open(
            el,
            selector_id,
            Rc::clone(&json_def_c),
            Rc::clone(&shadow_data_cursor)
        ) {
            Ok(maybe_data) => {
                if let Some(data_item) = maybe_data {
                    // Register end tag action immediatly
                    if el.can_have_content() { // if not, "on_end_tag" throws error
                        el.on_end_tag(move |end| {
                            ShadowData::on_data_tag_close(
                                end,
                                selector_id,
                                Rc::clone(&json_def_c),
                                Rc::clone(&shadow_data_cursor)
                            )?;
                            Ok(())
                        })?;
                    }
                    let self_weak = Rc::downgrade(&data_item);
                    let data_def = json_def_b.data.as_ref().unwrap(); // This should only be reached if data field had been set for this el
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
                                        if let Some(attr_value) = attrs.get(attr_name) {
                                            let mut new_data_m = data_item.borrow_mut();
                                            new_data_m.set(key, ShadowData::wrap(ShadowData::new_string(
                                                Some(selector_id),
                                                Weak::clone(&self_weak),
                                                attr_value.clone())
                                            ));
                                        }
                                    },
                                    ShadowJsonValueSource::Contents => {
                                        // This is handled by text_content_handler
                                    },
                                    ShadowJsonValueSource::Value => {
                                        // Fetch the current value from the different form elements
                                        match el.tag_name().as_str() {
                                            /* LOLHTML does not allow to operate on children, so to provide "select" shortcut we would need to create a new handler its children
                                            * However whether the element is select or not is unknown before parsing the element itself, and it is too late to add
                                            * a new handler at that point. So we cannot provide "select" shortcut. Instead use directly "select > option[selected=selected]"
                                            "select" => {},
                                            */
                                            "input" => {
                                                if let Some(input_type) = attrs.get("type") {
                                                    let mut new_data_m = data_item.borrow_mut();
                                                    match input_type.as_str() {
                                                        "radio" => {
                                                            if attrs.get("checked").is_some() {
                                                                // For radio/checkbox, we only consider the box which is checked. Make sure def json contains all items
                                                                new_data_m.set(key, ShadowData::wrap(
                                                                    ShadowData::new_string(Some(selector_id), Weak::clone(&self_weak), attrs.get("value")
                                                                    .unwrap_or(&String::from(""))
                                                                    .to_owned())
                                                                ));
                                                            } else if new_data_m.get(key).is_none() {
                                                                // Init
                                                                new_data_m.set(key, ShadowData::wrap(
                                                                    ShadowData::new_string(Some(selector_id), Weak::clone(&self_weak), "".to_string())
                                                                ));
                                                            }
                                                        }
                                                        "checkbox" => {
                                                            if new_data_m.get(key).is_none() {
                                                                new_data_m.set(key, ShadowData::wrap(
                                                                    ShadowData::new_array(Some(selector_id), Weak::clone(&self_weak))
                                                                ));
                                                            }
                                                            if attrs.get("checked").is_some() {
                                                                // For radio/checkbox, we only consider the box which is checked. Make sure def json contains all items
                                                                if let Some(arr) = new_data_m.get(key) {
                                                                    let mut arr_borrowed = arr.borrow_mut();
                                                                    arr_borrowed.push(ShadowData::wrap(
                                                                        ShadowData::new_string(Some(selector_id), Weak::clone(&self_weak), attrs.get("value")
                                                                        .unwrap_or(&String::from(""))
                                                                        .to_owned())
                                                                    ));
                                                                }
                                                            }
                                                        }
                                                        _ => {
                                                            new_data_m.set(key, ShadowData::wrap(
                                                                ShadowData::new_string(Some(selector_id), Weak::clone(&self_weak), attrs.get("value")
                                                                .unwrap_or(&String::from("").to_string())
                                                                .to_owned())
                                                            ));
                                                        }
                                                    }
                                                }
                                            },
                                            "option" => {
                                                let mut new_data_m = data_item.borrow_mut();
                                                new_data_m.set(key, ShadowData::wrap(
                                                    ShadowData::new_string(Some(selector_id), Weak::clone(&self_weak), attrs.get("value")
                                                    .unwrap_or(&String::from("")
                                                    .to_string()).to_owned())
                                                ));
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
            },
            Err(err) => {
                errors.borrow_mut().push(err.to_string());
            },
        }
        if delete {
            el.remove();
        }

        Ok(())
    }

    // Applies a regex to old_value and replaces with new_value
    // First access regex will be cached
    // Return None if no matches or error computing the regex
    fn match_replace<'a>(
        r#match: &'a String,
        old_value: &'a String,
        new_value: &'a String,
        errors: Rc<RefCell<Vec<String>>>,
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>
    ) -> Option<Cow<'a, str>> {
        let mut cache_borrowed = cache.borrow_mut();
        let regex_map: &mut HashMap<String, Regex> = cache_borrowed
            .get_mut("regex_map")
            .unwrap() // Instantiated during cache creation
            .downcast_mut::<HashMap<String, Regex>>()
            .unwrap(); // The type is known and fixed
        let mut regex_not_computed = regex_map.get(r#match).is_none();
        if regex_not_computed {
            // Not cached. Attempt to compute regex and cache it
            regex_not_computed = match Regex::new(r#match) {
                Ok(r_computed) => {
                    regex_map.insert(r#match.to_string(), r_computed);
                    false
                },
                Err(e) => {
                    errors.borrow_mut().push(format!("Invalid regex: {} | Error: {}", r#match, e));
                    true
                },
            }
        }
        if !regex_not_computed { // If still not computed => There was an error during computation. In that case do nothing
            let regex = regex_map.get(r#match).unwrap(); // We are certain it must exist now
            let new_val = regex.replace_all(
                old_value,
                new_value
            ); // If no match, replace returns the original old_value
            if &new_val != old_value {
                return Some(new_val)
            }
        }
        None
    }

    fn text_content_handler(
        el: &mut TextChunk,
        selector_id: usize,
        json_def: Rc<RefCell<ShadowJson>>,
        errors: Rc<RefCell<Vec<String>>>,
        content_buffer: Rc<RefCell<String>>,
        cache: Rc<RefCell<HashMap<String, Box<dyn Any>>>>,
        shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json_def_b = json_def.borrow();
        let mut content_buffer_b = content_buffer.borrow_mut();
        content_buffer_b.push_str(el.as_str()); // Saved chunk to buffer
        el.remove();
        if el.last_in_text_node() {
            // Last text chunk reached : process the buffer, send it back and reset it
            // PROCESSING BEGINS
            if let Some(edit) = &json_def_b.edit {
                if let Some(content) = &edit.content {
                    match content.op.as_str() {
                        "delete" => {
                            *content_buffer_b = String::new();
                        }
                        "upsert" => {
                            if let Some(value) = &content.val {
                                *content_buffer_b = value.clone();
                            } else {
                                let mut errors_m = errors.borrow_mut();
                                errors_m.push(format!("Upsert requires an existing val content string"));
                            }
                        }
                        "match_replace" => {
                            if let Some(r#match) = &content.r#match {
                                if let Some(new_value) = &content.val {
                                    if let Some(replacement) = Self::match_replace(
                                        r#match,
                                        &content_buffer_b,
                                        new_value,
                                        Rc::clone(&errors),
                                        Rc::clone(&cache)
                                    ) {
                                        *content_buffer_b = replacement.to_string();
                                    }
                                }
                            }
                        }
                        other => {
                            let mut errors_m = errors.borrow_mut();
                            errors_m.push(format!("Invalid operation (edit.content): {}. Allowed values : delete/upsert/match_replace", other));
                        }
                    }
                }
            }
            if let Some(data_def) = &json_def_b.data {
                let data = &shadow_data_cursor.borrow().shadow_data;
                let parent = Rc::downgrade(&data);
                if let Some(values) = &data_def.values {
                    if !values.is_empty() {
                        for (key, value) in values.iter() {
                            match value {
                                ShadowJsonValueSource::Contents => {
                                        let mut new_data_m = data.borrow_mut();
                                        new_data_m.set(key, ShadowData::wrap(
                                            ShadowData::new_string(Some(selector_id), Weak::clone(&parent), content_buffer_b.clone())
                                        ));
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
            el.replace(&content_buffer_b, ContentType::Text);
            content_buffer_b.clear(); // Reset
        }
        Ok(())
    }

    fn data_content_handler(
        data_formatter: Rc<Box<dyn Fn(String) -> String>>,
        ech: &mut Vec<(Cow<Selector>, ElementContentHandlers)>,
        shadow_data_cursor: Rc<RefCell<ShadowDataCursor>>
    ) {
        ech.push((
            Cow::Owned("body".parse().unwrap()),
            ElementContentHandlers::default().element(move |el| {
                let data = Rc::clone(&shadow_data_cursor.borrow().root);
                let data_formatter_c = Rc::clone(&data_formatter);
                let data_c = Rc::clone(&data);
                el.on_end_tag(move |end| {
                    let data_b = data_c.borrow_mut();
                    let props_html: String = (data_formatter_c)(data_b.to_string());
                    end.before(props_html.as_str(), ContentType::Html);
                    Ok(())
                })?;
                Ok(())
            })
        ));
    }

    pub fn process_json<W>(
        &self,
        writer : &mut W
    ) -> Result<(), ShadowError>
    where
        W: Write
    {
        let data = Rc::clone(&self.shadow_data_cursor.borrow().root);
        let data_str = data.borrow().to_string();
        // Write string chunk by chunk
        for chunk in data_str
            .bytes().collect::<Vec<u8>>()
            .chunks(self.max_chunk_bytesize) {
                if let Err(e) = writer.write(chunk) {
                    return Err(ShadowError { msg: format!("Error writing to client body : {}",e) });
                }
            }
        Ok(())
    }

    pub fn finalize_rewriter<'a, W: Write>(
        &self,
        writer: &'a mut W,
        errors: Rc<RefCell<Vec<String>>>
    ) -> HtmlRewriter<impl OutputSink + 'a>
    {
        let ech = self.ech.take(); // This is the last time we use ech, so we can remove it
        let as_json = self.options.and_then(|opts| Some(opts.as_json)).unwrap_or(false);
        let max_byte_chunksize = self.max_chunk_bytesize;
        
        HtmlRewriter::new(
            Settings {
                element_content_handlers: ech,
                ..Settings::default()
            },
            move |c: &[u8]| {
                if !as_json {
                    for chunk in c.chunks(max_byte_chunksize) { // Setting upper limit to writable chunk size
                        if let Err(e) = writer.write(chunk) {
                            Rc::clone(&errors).borrow_mut().push(format!("Error writing to client body : {}",e));
                        }
                    }
                } else {
                    // Discard HTML data, no write
                }
            }
        )
    }

    // Process providing just the reader, and use shadowapi's default chunk size
    pub fn process_html<W, R>(
        &self,
        writer: &mut W,
        reader: &mut R,
        errors: Rc<RefCell<Vec<String>>>
    )
    where
        W: Write,
        R: Read
    {
        let as_json = self.options.and_then(|opts| Some(opts.as_json)).unwrap_or(false);
        let mut rewriter = self.finalize_rewriter(writer, Rc::clone(&errors));
        let mut buf: [u8; MAX_CHUNK_BYTESIZE] = [0; MAX_CHUNK_BYTESIZE];
        loop {
            match reader.read(&mut buf) {
                Ok(n_bytes) => {
                    if n_bytes > 0 {
                        if let Err(err) =  rewriter.write(&buf[0..n_bytes]) {
                            errors.borrow_mut().push(format!("[process_html] write err : {}", err.to_string()));
                        }
                    } else {
                        break; // Writing complete
                    }
                },
                Err(err) => {
                    errors.borrow_mut().push(format!("[process_html] read error : {}", err.to_string()));
                },
            }
        }
        if let Err(err) = rewriter.end() {
            errors.borrow_mut().push(format!("Error ending the rewriter : {}", err.to_string()));
        }
        if as_json {
            if let Err(err) = self.process_json(
                writer
            ) {
                errors.borrow_mut().push(format!("[process_json] {}", err.to_string()));
            }
        }
    }

    // Process using a chunk iterator instead of a reader, allowing to specify custom bytesize
    pub fn process_html_iter<W, I>(
        &self,
        writer: &mut W,
        chunk_iter: &mut I,
        errors: Rc<RefCell<Vec<String>>>
    )
    where
        W: Write,
        I: Iterator<Item = Result<Vec<u8>, std::io::Error>>
    {
        let as_json = self.options.and_then(|opts| Some(opts.as_json)).unwrap_or(false);
        let mut rewriter = self.finalize_rewriter(writer, Rc::clone(&errors));

        for chunk in chunk_iter {
            if let Ok(chunk_data) = chunk {
                if let Err(e) = rewriter.write(&chunk_data) {
                    errors.borrow_mut().push(format!("[process_html_iter] write error : {}", e));
                    return;
                }
            } else if let Err(err) = chunk {
                errors.borrow_mut().push(format!("[process_html_iter] invalid chunk : {}", err.to_string()));
                return;
            }
        }
        if let Err(err) = rewriter.end() {
            errors.borrow_mut().push(format!("[process_html_iter] rewriter not ending : {}", err.to_string()));
            return;
        }
        if as_json {
            if let Err(err) = self.process_json(
                writer
            ) {
                errors.borrow_mut().push(format!("[process_json] error : {}", err.to_string()));
            }
        }
    }
}

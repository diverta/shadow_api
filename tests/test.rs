use std::io::BufWriter;
use std::{rc::Rc, cell::RefCell};
use shadow_api::{ShadowJson, ShadowApiReplacer, ShadowApiInit};
use shadow_api::ShadowApi;

thread_local! {
    // The object holding the closure needs to be static (global), as the closure is invoked after process function exists, as it done later in async manner
    pub static REPLACER: RefCell<Option<ShadowApiReplacer<'static>>> = RefCell::new(None);
}


fn html_source<'a>() -> &'a str {
    r##"<html>
<head>
  <title>Old Title</title>
  <meta name="meta1" content="meta1 content">
  <meta name="meta2" content="meta2 content">
</head>
<body>
  <div name="match_test">Apple Banana</div>
  <a class="top_link" href="https://top.link" style="display:none">TopLink</a>
  <div class="to_delete">First item to be deleted</div>
  <div id="first">
    <form>
      <input type="text" name="text_key" value="text_val" />
      <div class="radio">
        <input type="radio" name="radio_key" value="radio_val_unchecked" />
        <input type="radio" name="radio_key" value="radio_val_checked" checked />
      </div>
      <div class="chkbox">
        <input type="checkbox" name="checkbox_key" value="1" checked />
        <input type="checkbox" name="checkbox_key" value="2" />
        <input type="checkbox" name="checkbox_key" value="3" checked />
      </div>
      <select name="select_key">
        <option name="select_opt1" value="select_val1">Select Value 1</option>
        <option name="select_opt2" value="select_val2" selected="selected">Select Value 2</option>
        <option name="select_opt2" value="select_val3">Select Value 3</option>
    </form>
    <div class="to_delete">Second item to be deleted</div>
  </div>
  <div id="second">
    <div id="el_anchor">Anchor</div>
  </div>
  <div class="to_delete">Third item to be deleted</div>
  <div id="collections">
    <div class="coll1">
        <a href="coll1_link1">Coll1 Title1</a>
    </div>
    <div class="coll1">
        <a href="coll1_link2">Coll1 Title2</a>
    </div>
    <div class="coll2">
        <a href="coll2_link1">Coll2 Title1</a>
    </div>
    <div class="coll2">
        <a href="coll2_link2">Coll2 Title2</a>
    </div>
  </div>
</body>
</html>"##
}

fn html_result<'a>() -> &'a str {
    r##"<html>
<head>
  <title>New Title</title>
  <meta name="meta1" content="just meta1">
  <meta name="meta2" content="just meta2">
</head>
<body>
  <div name="match_test">Banana Apple</div>
  <a class="top_link" href="https://top.link" id="123">New Top Link</a>
  
  <div id="first">
    <form>
      <input type="text" name="text_key" value="text_val" />
      <div class="radio">
        <input type="radio" name="radio_key" value="radio_val_unchecked" />
        <input type="radio" name="radio_key" value="radio_val_checked" checked />
      </div>
      <div class="chkbox">
        <input type="checkbox" name="checkbox_key" value="1" checked />
        <input type="checkbox" name="checkbox_key" value="2" />
        <input type="checkbox" name="checkbox_key" value="3" checked />
      </div>
      <select name="select_key">
        <option name="select_opt1" value="select_val1">Select Value 1</option>
        <option name="select_opt2" value="select_val2" selected="selected">Select Value 2</option>
        <option name="select_opt2" value="select_val3">Select Value 3</option>
    </form>
    
  </div>
  <div id="second">
    <div>Insert Before</div><div id="el_anchor"><div>Prepend</div>Anchor<div>AppendModified</div></div><div>Insert After</div>
  </div>
  
  <div id="collections">
    <div class="coll1">
        <a href="coll1_link1">Coll1 Title1</a>
    </div>
    <div class="coll1">
        <a href="coll1_link2">Coll1 Title2</a>
    </div>
    <div class="coll2">
        <a href="coll2_link1">Coll2 Title1</a>
    </div>
    <div class="coll2">
        <a href="coll2_link2">Coll2 Title2</a>
    </div>
  </div>
<script>var my_data = {"top_link":{"url":"https://top.link","name":"New Top Link"},"to_delete":[{"contents":"First item to be deleted"},{"contents":"Second item to be deleted"},{"contents":"Third item to be deleted"}],"formdata":{"text_key":"text_val","radio_key":"radio_val_checked","checkbox_key":["1","3"],"select_key":"select_val2"},"coll1":[{"href":"coll1_link1","name":"Coll1 Title1"},{"href":"coll1_link2","name":"Coll1 Title2"}],"coll2":[{"href":"coll2_link1","name":"Coll2 Title1"},{"href":"coll2_link2","name":"Coll2 Title2"}]};</script></body>
</html>"##
}

fn shadow_json_1<'a>() -> &'a str {
    r##"
    {
        "s": "head",
        "sub": [
            {
                "s": "title",
                "edit": {
                    "content": {
                        "op": "upsert",
                        "val": "New Title"
                    }
                }
            },
            {
                "s": "meta",
                "edit": {
                    "attrs": {
                        "content": {
                            "op": "match_replace",
                            "match": "^(.*) content$",
                            "val": "just $1"
                        }
                    }
                }
            }
        ]
    }
    "##
}

fn shadow_json_2<'a>() -> &'a str {
    r##"
    {
        "s": "body",
        "sub": [
            {
                "s": "div[name=\"match_test\"]",
                "edit": {
                    "content": {
                        "op": "match_replace",
                        "match": "(\\S+) (\\S+)",
                        "val": "$2 $1"
                    }
                }
            },
            {
                "s": "a.top_link",
                "edit": {
                    "attrs": {
                        "style": {
                            "op": "delete"
                        },
                        "id": {
                            "op": "upsert",
                            "val": "123"
                        }
                    },
                    "content": {
                        "op": "upsert",
                        "val": "New Top Link"
                    }
                },
                "data": {
                    "path": "top_link",
                    "values": {
                        "url": {"source": "Attribute", "name":"href"},
                        "name": {"source": "Contents"}
                    }
                },
                "hide": true
            },
            {
                "s": "#first form",
                "data": {
                    "path": "formdata"
                },
                "sub": [
                    {
                        "s": "input[name=\"text_key\"]",
                        "data": {
                            "values": {
                                "text_key": {"source": "Value"}
                            }
                        }
                    },
                    {
                        "s": "input[name=\"radio_key\"][checked]",
                        "data": {
                            "values": {
                                "radio_key": {"source": "Value"}
                            }
                        }
                    },
                    {
                        "s": "input[name=\"checkbox_key\"][checked]",
                        "data": {
                            "values": {
                                "checkbox_key": {"source": "Value"}
                            }
                        }
                    },
                    {
                        "s": "select[name=\"select_key\"] > option[selected=\"selected\"]",
                        "data": {
                            "values": {
                                "select_key": {"source": "Value"}
                            }
                        }
                    }
                ]
            },
            {
                "s": "#second > #el_anchor",
                "append": ["<div>Append</div>"],
                "prepend": ["<div>Prepend</div>"],
                "insert_before": ["<div>Insert Before</div>"],
                "insert_after": ["<div>Insert After</div>"]
            },
            {
                "s": ".to_delete",
                "data": {
                    "path": "to_delete.",
                    "values": {
                        "contents": {"source": "Contents"}
                    }
                },
                "delete": true
            },
            {
                "s": ".coll1",
                "data": {
                    "path": "coll1."
                },
                "sub": [
                    {
                        "s": "a",
                        "data": {
                            "values": {
                                "href": {"source": "Attribute", "name": "href"},
                                "name": {"source": "Contents"}
                            }
                        }
                    }
                ]
            },
            {
                "s": ".coll2",
                "sub": [
                    {
                        "s": "a",
                        "data": {
                            "path": "coll2.",
                            "values": {
                                "href": {"source": "Attribute", "name": "href"},
                                "name": {"source": "Contents"}
                            }
                        }
                    }
                ]
            }
        ]
    }
    "##
}

#[test]
fn test() {
    let html_source = html_source();

    let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let json_def: Rc<Vec<Rc<RefCell<ShadowJson>>>> = Rc::new(Vec::from([
    // First ShadowJson
    Rc::new(RefCell::new(ShadowJson::parse_str(shadow_json_1(), Rc::clone(&errors)))),
    // Second ShadowJson
    Rc::new(RefCell::new(ShadowJson::parse_str(shadow_json_2(), Rc::clone(&errors))))],));


    let mut output = BufWriter::new(Vec::new());
    let mut shadow_api_o = ShadowApi::new(None);
    shadow_api_o.set_data_formatter(Rc::new(Box::new(move |data: String| {
        format!("<script>var my_data = {};</script>", data)
    })));

    {
        // Testing ShadowJson string transform
        let second_shadowjson = Rc::clone(Rc::clone(&json_def).get(1).unwrap());
        let mut second_shadowjson = second_shadowjson.borrow_mut();
        second_shadowjson.transform_strings(&mut |s: &mut String| {
            *s = s.replace("Append", "AppendModified");
        });
    }

    shadow_api_o.parse(json_def, Rc::clone(&errors));

    let chunk_size = 100;
    let mut bytes = html_source.as_bytes().chunks(chunk_size).map(|c| { Ok(c.to_vec())});
    shadow_api_o.process_html_iter(
        &mut output,
        &mut bytes,
        Rc::clone(&errors)
    );
    drop(shadow_api_o);

    println!("Errors: {:#?}", errors);
    assert_eq!(Rc::clone(&errors).borrow().len(), 0);

    let bytes = output.into_inner().unwrap_or_default();

    let processed_html = String::from_utf8(bytes).unwrap_or("<UTF8 ERROR>".to_string());
    let expected_html_output = html_result();

    assert_eq!(processed_html,expected_html_output);
}

#[test]
fn test_replacer<'a>() {
    let html_source: &'a str = html_source(); "<html><head><title>Old title</title></head><body></body></html>";

    let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let json_def: Vec<Rc<RefCell<ShadowJson>>> = Vec::from([
        // First ShadowJson
        Rc::new(RefCell::new(ShadowJson::parse_str(shadow_json_1(), Rc::clone(&errors)))),
        // Second ShadowJson
        Rc::new(RefCell::new(ShadowJson::parse_str(shadow_json_2(), Rc::clone(&errors))))
    ]);

    {
        // Testing ShadowJson string transform
        let second_shadowjson = Rc::new(&json_def).get(1).unwrap();
        let mut second_shadowjson = second_shadowjson.borrow_mut();
        second_shadowjson.transform_strings(&mut |s: &mut String| {
            *s = s.replace("Append", "AppendModified");
        });
    }

    let shadow_api_init = ShadowApiInit::new(
        None,
        8196,
        Box::new(|data: String| {
            format!("<script>var my_data = {};</script>", data)
        }),
        json_def,
        errors
    );

    REPLACER.with(|replacer| {
        let shadow_api_o = shadow_api_init.init();
        let mut rb = replacer.borrow_mut();
        *rb = Some(shadow_api_o.finalize_replacer());
    });

    let mut output: Vec<u8> = Vec::new();
    let expected_html_output = html_result();
    
    // Testing various chunk sizes
    let source_bytes = html_source.as_bytes();
    for chunk in source_bytes.chunks(10) { // Any chunk size would work
        REPLACER.with(|replacer| {
            let mut rb = replacer.borrow_mut();
            let rb = rb.as_mut().unwrap();
            let (replaced, written) = rb.replace(chunk).unwrap();
            let replaced = replaced.borrow();
            let replaced_slice = replaced.as_slice();
            output.extend(replaced_slice[..written].iter());
        });
    }
    let processed_html_output = String::from_utf8(output).unwrap();
    assert_eq!(processed_html_output, expected_html_output);
}
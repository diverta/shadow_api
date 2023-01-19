use std::io::{BufWriter};
use std::{rc::Rc, cell::RefCell};
use shadow_api::ShadowJson;
use shadow_api::ShadowApi;

#[test]
fn test() {
    let html_source = r##"<html>
<head>
</head>
<body>
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
</html>"##;

let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
let json_def: Rc<Vec<Rc<RefCell<ShadowJson>>>> = Rc::new(Vec::from([Rc::new(RefCell::new(ShadowJson::parse_str(r##"
{
    "s": "body",
    "sub": [
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
"##, Rc::clone(&errors))))]));

    let mut shadow_api_o = ShadowApi::new();
    shadow_api_o.set_data_formatter(Rc::new(Box::new(move |data: String| {
        format!("<script>var my_data = {};</script>", data)
    })));

    shadow_api_o.parse(Rc::clone(&json_def), Rc::clone(&errors));

    {
        // Testing ShadowJson string transform
        let first_shadowjson = Rc::clone(Rc::clone(&json_def).get(0).unwrap());
        let mut first_shadowjson = first_shadowjson.borrow_mut();
        first_shadowjson.transform_strings(&mut |s: &mut String| {
            *s = s.replace("Append", "AppendModified");
        });
    }

    let mut output = BufWriter::new(Vec::new());

    let chunk_size = 50;
    let mut bytes = html_source.as_bytes().chunks(chunk_size).map(|c| { Ok(c.to_vec())});
    shadow_api_o.process_html(&mut output, &mut bytes, Rc::clone(&errors));

    println!("Erros: {:#?}", errors);
    assert_eq!(Rc::clone(&errors).borrow().len(), 0);

    let bytes = output.into_inner().unwrap_or_default();

    let processed_html = String::from_utf8(bytes).unwrap_or("<UTF8 ERROR>".to_string());
    println!("PROCESSED: {}", processed_html);
    let expected_html_output = r##"<html>
<head>
</head>
<body>
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
<script>var my_data = {"top_link":{"url":"https://top.link","name":"New Top Link"},"formdata":{"text_key":"text_val","radio_key":"radio_val_checked","checkbox_key":["1","3"],"select_key":"select_val2"},"to_delete":[{"contents":"Third item to be deleted"},{"contents":"Third item to be deleted"},{"contents":"Third item to be deleted"}],"coll1":[{"href":"coll1_link2","name":"Coll1 Title2"},{"href":"coll1_link2","name":"Coll1 Title2"}],"coll2":[{"href":"coll2_link2","name":"Coll2 Title2"},{"href":"coll2_link2","name":"Coll2 Title2"}]};</script></body>
</html>"##;

    assert_eq!(processed_html,expected_html_output);
}
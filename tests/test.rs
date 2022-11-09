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
  <a href="https://top.link">TopLink</a>
  <div class="to_delete">First item to be deleted</div>
  <div id="first">
    <form>
      <input type="text" name="text_key" value="text_val" />
      <div class="radio">
        <input type="radio" name="radio_key" value="radio_val_unchecked" />
        <input type="radio" name="radio_key" value="radio_val_checked" checked />
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
</body>
</html>"##;

let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
let json_def: Rc<Vec<Rc<ShadowJson>>> = Rc::new(Vec::from([Rc::new(ShadowJson::parse_str(r##"
{
    "s": "body",
    "sub": [
        {
            "s": "a",
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
            "delete": true
        }
    ]
}
"##, Rc::clone(&errors)))]));

    let mut shadow_api_o = ShadowApi::new();
    shadow_api_o.set_data_formatter(Rc::new(Box::new(move |data: String| {
        format!("<script>var my_data = {};</script>", data)
    })));

    shadow_api_o.parse(json_def, Rc::clone(&errors));

    let mut output = BufWriter::new(Vec::new());

    let chunk_size = 50;
    let mut bytes = html_source.as_bytes().chunks(chunk_size).map(|c| { Ok(c.to_vec())});
    shadow_api_o.process_html(&mut output, &mut bytes, Rc::clone(&errors));

    let bytes = output.into_inner().unwrap_or_default();

    let processed_html = String::from_utf8(bytes).unwrap_or("<UTF8 ERROR>".to_string());
    let expected_html_output = r##"<html>
<head>
</head>
<body>
  <a href="https://top.link" style="display: none">TopLink</a>
  
  <div id="first">
    <form>
      <input type="text" name="text_key" value="text_val" />
      <div class="radio">
        <input type="radio" name="radio_key" value="radio_val_unchecked" />
        <input type="radio" name="radio_key" value="radio_val_checked" checked />
      </div>
      <select name="select_key">
        <option name="select_opt1" value="select_val1">Select Value 1</option>
        <option name="select_opt2" value="select_val2" selected="selected">Select Value 2</option>
        <option name="select_opt2" value="select_val3">Select Value 3</option>
    </form>
    
  </div>
  <div id="second">
    <div>Insert Before</div><div id="el_anchor"><div>Prepend</div>Anchor<div>Append</div></div><div>Insert After</div>
  </div>
  
<script>var my_data = {"top_link":{"url":"https://top.link","name":"TopLink"},"formdata":{"text_key":"text_val","radio_key":"radio_val_checked","select_key":"select_val2"}};</script></body>
</html>"##;

    assert_eq!(processed_html,expected_html_output);
    assert_eq!(Rc::clone(&errors).borrow().len(), 0);
}
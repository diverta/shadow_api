## Description

ShadowAPI is a Rust library for efficiently processing streaming HTML on the fly, for applying modifications and collecting data at once. The data is appended at the end of the body as a JSON with customizeable formatting.

ShadowAPI is built upon [LOL HTML](https://github.com/cloudflare/lol-html) and uses [Serde JSON](https://github.com/serde-rs/json)

As input, you are required to build a JSON tree describing the actions you want to apply on the DOM (and marking the data to collect), optionally define the formatting and provide stream reader and writer. The main usecase is to use it with Fastly Compute@Edge or Cloudflare Workers for easy HTML processing and data retrieval, although it is free from these dependencies

## Example
Consider the following HTML

```html
<html>
    <head>
    </head>
    <body>
        <div id="content">
            <a href="https://en.wikipedia.org/wiki/Smallville"><span id="name">SmallVille</span></a>
            <form id="form">
                <input type="hidden" name="weakness" value="cryptonite">
                <input type="text" name="first_name" value="Clark">
                <input type="text" name="last_name" value="Kent">
            </div>
        </div>
    </body>
</html>"
```

Let's describe some modifications we want to apply to this form, as well as collect the data by building a ShadowJson structure : 
```rust
let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
let json_def: Rc<Vec<Rc<ShadowJson>>> = Rc::new(Vec::from([Rc::new(ShadowJson::parse_str(r##"
{
    "s": "#content",
    "data": {
        "path": "data"
    },
    "sub": [
        {
            "s": "a",
            "data": {
                "values": {
                    "wiki_link": {"source": "Attribute", "name":"href"}
                }
            },
            "sub": [
                {
                    "s": "#city",
                    "data": {
                        "values": {
                            "city_name": {"source": "Contents"}
                        }
                    }
                }
            ],
            "hide": true
        },
        {
            "s": "#form",
            "data": {
                "path": "input"
            },
            "sub": [
                {
                    "s": "input[name=first_name]",
                    "data": {
                        "values": {
                            "first_name": {"source": "Value"}
                        }
                    }
                },
                {
                    "s": "input[name=last_name]",
                    "data": {
                        "values": {
                            "family_name": {"source": "Value"}
                        }
                    }
                },
                {
                    "s": "input[name=weakness]",
                    "delete": true
                }
            ],
            "insert_after": ["<div>No weaknesses</div>"]
        }
    ]
}
"##, Rc::clone(&errors)))]));
```
`errors` is here to help with data validation, in case the Json you constructed contains mistakes. You may log it for debugging. It is deserialized into ShadowJson struct, check if for options. Hopefully it is self-explanatory enough : a tree structure representing the DOM structure : 
- `s` : Nested selectors are only selecting elements under their parent, as one might expect. 
- `data` : Optional, add it when you need to collect the data from the element under this selector
- `data.path` : Optional, specify a path to organize resulting data. If the path ends with a single dot `.`, then the elements will be appended to an array.
- `data.values` : Specify the key for data storage, and where to fetch it (under `source`). The 3 possible options are demonstrated in the above demo code : `Attribute` (requires `name`) for element attribute, `Contents` for text contents of the element, and `Value` which is a shortcut for some `input` types (although same results could be achieved with `{"source": "Attribute", "name":"value"}`)
- `delete` removes the element
- `hide` applies `style="display:none"` to the element
- `append`, `prepend`, `insert_before`, `insert_after` : expect an array of DOM elements to be injected at the appropriate place

Now, let's apply the ShadowApi. The following code uses Fastly::Response (example for `Compute@Edge`). Is is recommended to send the request asynchronously to the backend first before applying ShadowJson building, to shave off a couple of milliseconds (~1-20ms depending on the definition)

```rust
            let resp: fastly::http::request::PendingRequest = req.send_async(config.backend_name)?;

            // Insert here the above ShadowJson building code. You may want to fetch it with another API for a dynamic definition

            let mut shadow_api_o = ShadowApi::new(); // Instantiate
			
            shadow_api_o.set_data_formatter(Rc::new(Box::new(move |data: String| {
                format!("<script>Mail.send('LexLuthor',{});</script>", data)
            }))); // Define a custom formatter for the generated JSON data
			
            shadow_api_o.parse(json_def, Rc::clone(&errors)); // This crawls ShadowJson and builds all element and text content handlers for LOLHTML.

            let mut resp = resp.wait()?; // Only now start waiting
            let mut resp_body = resp.take_body();
            let mut client_body = resp.stream_to_client(); // Begin the stream back

            shadow_api_o.process_html(&mut client_body, &mut resp_body.read_chunks(CHUNK_SIZE), Rc::clone(&errors)); // This reads the chunk iterator over the body, and applies the processing chunk by chunk

            drop(client_body); // Response is sent, cleanup
```

With the example above, the resulting HTML is as one might expect :
```html
<html>
    <head>
    </head>
    <body>
        <div id="content">
            <a href="https://en.wikipedia.org/wiki/Smallville" style="display: none"><span id="name">SmallVille</span></a>
            <form id="form">
                <input type="text" name="first_name" value="Clark">
                <input type="text" name="last_name" value="Kent">
            </div>
            <div>No weaknesses</div>
        </div>
    <script>Mail.send('LexLuthor',{"data":{"wiki_link":"https://en.wikipedia.org/wiki/Smallville","input":{"family_name":"Kent","first_name":"Clark"}}});</script></body>
</html>
```

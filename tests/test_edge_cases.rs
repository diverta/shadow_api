use std::io::Write;
use std::{rc::Rc, cell::RefCell};
use shadow_api::{ShadowJson, ShadowApiInit};

#[test]
// Tests with upserting/replacements in the nodes which contain nested DOM
fn test_content_subtree() {
    let html_source: &str = r#"<h3 class="_16u2l0ua" style="overflow-wrap:anywhere;word-break:keep-all">世界を<wbr>リードする<wbr>デジタルイノベーターの<wbr>信頼を<wbr>得ています</h3>"#;
    let errors: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let json_def: Vec<Rc<RefCell<ShadowJson>>> = Vec::from([
        Rc::new(RefCell::new(ShadowJson::parse_str(
            r##"{
                "s": "h3._16u2l0ua",
                "edit": {
                    "content": {
                        "op": "upsert",
                        "val": "replaced"
                    }
                }
            }"##, Rc::clone(&errors)
        )))
    ]);

    let shadow_api_init = ShadowApiInit::new(
        None,
        8196,
        Box::new(|_: String| String::new()),
        json_def,
        Rc::clone(&errors)
    );

    let shadow_api = shadow_api_init.init();

    let mut output: Vec<u8> = Vec::new();
    let expected_html_output = "<h3 class=\"_16u2l0ua\" style=\"overflow-wrap:anywhere;word-break:keep-all\">replaced</h3>";
    
    let source_bytes = html_source.as_bytes();

    let mut shadow_api_rewriter = shadow_api.finalize_rewriter(&mut output, errors);
    shadow_api_rewriter.write_all(source_bytes).unwrap();
    drop(shadow_api_rewriter);
    
    let processed_html_output = String::from_utf8(output).unwrap();
    assert_eq!(processed_html_output, expected_html_output);
}
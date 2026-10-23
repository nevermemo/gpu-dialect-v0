use super::{Json, parse};

fn string(text: &str) -> Json {
    Json::String(text.to_owned())
}

#[test]
fn every_value_kind_is_read() {
    let value = parse(
        r#" { "null": null, "yes": true, "no": false, "int": -12, "real": 1.5e+3,
              "text": "a", "list": [1, "two", [], {}], "object": {"k": "v"} } "#,
    )
    .unwrap();
    assert_eq!(value.get("null"), Some(&Json::Null));
    assert_eq!(value.get("yes"), Some(&Json::Bool(true)));
    assert_eq!(value.get("no"), Some(&Json::Bool(false)));
    assert_eq!(value.get("int"), Some(&Json::Number("-12".to_owned())));
    assert_eq!(value.get("real"), Some(&Json::Number("1.5e+3".to_owned())));
    assert_eq!(value.get("text"), Some(&string("a")));
    let list = value.get("list").and_then(Json::as_array).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[2], Json::Array(vec![]));
    assert_eq!(list[3], Json::Object(vec![]));
    assert_eq!(
        value.get("object").and_then(|o| o.get("k")),
        Some(&string("v"))
    );
    assert_eq!(value.get("missing"), None);
    // `get` on a non-object, and the string helpers on non-strings.
    assert_eq!(list[1].get("k"), None);
    assert_eq!(value.get("null").and_then(Json::as_str), None);
    assert_eq!(value.get("null").and_then(Json::string_or_null), None);
    assert_eq!(
        value.get("text").and_then(Json::string_or_null).as_deref(),
        Some("a")
    );
}

#[test]
fn a_repeated_key_reads_as_its_last_value() {
    let value = parse(r#"{"a": 1, "a": 2}"#).unwrap();
    assert_eq!(value.get("a"), Some(&Json::Number("2".to_owned())));
}

#[test]
fn string_escapes_are_decoded() {
    let value = parse(r#""q\" b\\ s\/ \b\f\n\r\t \u0041 \u00e9 \ud83d\ude00 end""#).unwrap();
    assert_eq!(
        value,
        string("q\" b\\ s/ \u{8}\u{c}\n\r\t A \u{e9} \u{1F600} end")
    );
    // Raw multi-byte text passes through untouched, next to an escape.
    assert_eq!(
        parse("\"h\u{e9}llo \\n \u{1F600}\"").unwrap(),
        string("h\u{e9}llo \n \u{1F600}")
    );
}

#[test]
fn malformed_text_is_rejected_with_a_position() {
    for text in [
        "",
        "   ",
        "nul",
        "tru",
        "{",
        "[1,",
        "[1 2]",
        "[1,]",
        "{\"a\" 1}",
        "{\"a\": 1,}",
        "{a: 1}",
        "{\"a\": }",
        "\"open",
        "\"bad \\q escape\"",
        "\"short \\u12\"",
        "\"lone \\ud83d\"",
        "\"lone \\ud83d x\"",
        "\"high \\ud83d\\u0041\"",
        "\"low \\ude00\"",
        "\"tab\tinside\"",
        "01",
        "-",
        "1.",
        "1e",
        "+1",
        "1 2",
        "{} extra",
        "'single'",
    ] {
        let error = parse(text).unwrap_err();
        assert!(
            error.starts_with("invalid JSON at byte "),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn absurd_nesting_is_an_error_not_a_stack_overflow() {
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(parse(&deep).unwrap_err().contains("too deep"));
    let fine = format!("{}{}", "[".repeat(100), "]".repeat(100));
    assert!(parse(&fine).is_ok());
}

use super::Bru;

const REQUEST: &str = "meta {
  name: Create cart
  type: http
  seq: 1
}

post {
  url: {{baseUrl}}/carts?ref=a:b
  body: json
  auth: inherit
}

headers {
  Content-Type: application/json
  ~Accept: application/json
}

body:json {
  {
    \"currency\": \"{{currency}}\"
  }
}

vars:post-response {}
";

fn parsed(text: &str) -> Bru {
    Bru::parse(text).unwrap_or_else(|reason| panic!("{reason}"))
}

#[test]
fn a_dictionary_block_reads_each_key_up_to_its_first_colon() {
    let bru = parsed(REQUEST);
    assert_eq!(bru.value("meta", "name").as_deref(), Some("Create cart"));
    assert_eq!(bru.value("post", "url").as_deref(), Some("{{baseUrl}}/carts?ref=a:b"));
    assert_eq!(bru.value("post", "missing"), None);
    assert_eq!(bru.value("delete", "url"), None);
}

#[test]
fn a_key_written_with_a_tilde_is_off() {
    let headers = parsed(REQUEST).pairs("headers");
    let read: Vec<_> = headers.iter().map(|pair| (pair.name.as_str(), pair.value.as_str(), pair.enabled)).collect();
    assert_eq!(read, [("Content-Type", "application/json", true), ("Accept", "application/json", false)]);
    assert_eq!(parsed(REQUEST).value("headers", "Accept"), None);
}

#[test]
fn a_text_block_is_outdented_by_the_two_spaces_bruno_indents_it_by() {
    let bru = parsed(REQUEST);
    assert_eq!(bru.text("body:json").as_deref(), Some("{\n  \"currency\": \"{{currency}}\"\n}"));
    assert!(bru.has("vars:post-response"));
    assert_eq!(bru.text("vars:post-response").as_deref(), Some(""));
    assert_eq!(bru.text("tests"), None);
}

#[test]
fn a_list_block_reads_comma_separated_names() {
    let bru = parsed("vars {\n  baseUrl: http://x\n}\nvars:secret [\n  apiToken,\n  ~payKey\n]\n");
    let names: Vec<_> = bru.list("vars:secret").iter().map(|pair| (pair.name.clone(), pair.enabled)).collect();
    assert_eq!(names, [("apiToken".to_owned(), true), ("payKey".to_owned(), false)]);
    assert_eq!(bru.value("vars", "baseUrl").as_deref(), Some("http://x"));
}

#[test]
fn a_block_with_no_closer_is_refused_without_quoting_the_file() {
    let refused = Bru::parse("headers {\n  X-Token: tok_Secret\n").err();
    assert_eq!(refused.as_deref(), Some("its headers block has no closing }"));
}

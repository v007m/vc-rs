use super::{Request, request};
use crate::curl::shell::{commands, words};

fn read(command: &str) -> Request {
    let words = words(command).unwrap_or_default();
    commands(&words).first().map(|args| request(args)).unwrap_or_default()
}

fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

#[test]
fn a_copied_command_gives_its_method_url_headers_and_data() {
    let copied = "curl -X PATCH https://a.test/users/u_7f3k -H 'content-type: application/json' \
                  -H 'accept:*/*' --data-raw '{\"name\":\"Ada\"}' --compressed -sSL --max-time 5";
    let expected = Request {
        method: Some("PATCH".to_owned()),
        url: Some("https://a.test/users/u_7f3k".to_owned()),
        headers: owned(&[("content-type", "application/json"), ("accept", "*/*")]),
        data: vec!["{\"name\":\"Ada\"}".to_owned()],
        ..Request::default()
    };
    assert_eq!(read(copied), expected);
}

#[test]
fn short_options_bundle_and_take_the_rest_of_their_word() {
    let read = read("curl -sXPOST -Hx:1 --url https://a.test -d a=1 -d@body.json -G -I -F f=1 --json '{}'");
    assert_eq!((read.method.as_deref(), read.url.as_deref()), (Some("POST"), Some("https://a.test")));
    assert_eq!(read.headers, owned(&[("x", "1")]));
    assert_eq!((read.data, read.form), (vec!["a=1".to_owned(), "{}".to_owned()], vec!["f=1".to_owned()]));
    assert!(read.get && read.head && read.json);
    assert_eq!(read.notes, ["-d @body.json: the file it sends is not read"]);
}

#[test]
fn data_urlencode_encodes_the_content_and_keeps_the_name() {
    let read = read("curl a --data-urlencode 'q=a b&c' --data-urlencode =é --data-urlencode plain");
    assert_eq!(read.data, ["q=a%20b%26c", "%C3%A9", "plain"]);
}

#[test]
fn credentials_are_named_and_their_values_never_kept() {
    let read = read("curl a -u ada:s3cret -b 'sid=t0ken' --oauth2-bearer b3arer -U p:x");
    assert_eq!(read.credentials, ["-u", "-b", "--oauth2-bearer", "--proxy-user"]);
    let kept = format!("{read:?}");
    assert!(["s3cret", "t0ken", "b3arer", "p:x"].iter().all(|value| !kept.contains(value)), "{kept}");
}

#[test]
fn what_cannot_be_read_is_noted() {
    let read = read("curl a b --frobnicate -Z -H nocolon -H");
    assert_eq!(read.url.as_deref(), Some("a"));
    let notes = [
        "a second URL is not read",
        "--frobnicate is not read",
        "-Z is not read",
        "a -H that is not `name: value` is not read",
        "-H has no value",
    ];
    assert_eq!(read.notes, notes);
}

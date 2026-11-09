use super::{Word, commands, words};

fn texts(text: &str) -> Vec<String> {
    words(text).unwrap_or_default().into_iter().map(|word| word.text).collect()
}

#[test]
fn words_split_at_blanks_and_lose_their_quotes_and_continuations() {
    let copied = "curl 'https://a.test/x?q=1' \\\n  -H \"accept: */*\" \\\r\n  --data-raw $'{\"n\":\"a\\'b\\\\nc\"}'";
    assert_eq!(
        texts(copied),
        ["curl", "https://a.test/x?q=1", "-H", "accept: */*", "--data-raw", "{\"n\":\"a'b\\nc\"}"]
    );
    assert_eq!(texts("a\"b c\"'d'\\ e ''"), ["ab cd e", ""]);
    assert_eq!(texts("# a comment\ncurl x # another\n-v"), ["\n", "curl", "x", "\n", "-v"]);
}

#[test]
fn escapes_are_read_as_bash_reads_them_in_each_quote() {
    assert_eq!(texts(r#""a\"b\$c\d\\""#), [r#"a"b$c\d\"#]);
    assert_eq!(texts("\"one \\\ntwo\""), ["one two"]);
    assert_eq!(texts(r"'a\nb'"), [r"a\nb"]);
    assert_eq!(texts(r"$'\t\x41é\U0001F600\q\x'"), ["\tAé😀\\q\\x"]);
    assert_eq!(texts("$x a$"), ["$x", "a$"]);
}

#[test]
fn an_unclosed_quote_is_an_error() {
    assert_eq!(words("curl 'a"), Err("a ' quote is not closed".to_owned()));
    assert_eq!(words("curl \"a"), Err("a \" quote is not closed".to_owned()));
    assert_eq!(words("curl $'a\\"), Err("a ' quote is not closed".to_owned()));
}

#[test]
fn each_unquoted_curl_starts_a_command_that_an_operator_ends() {
    let words =
        words("$ curl a?x=1&y -d 'curl'|jq .\ncurl b; echo\n'curl' c\ncurl d >out & curl e").unwrap_or_default();
    let args: Vec<Vec<&str>> =
        commands(&words).iter().map(|args| args.iter().map(|word| word.text.as_str()).collect()).collect();
    assert_eq!(args, [vec!["a?x=1&y", "-d", "curl"], vec!["b"], vec!["d"], vec!["e"]]);
    assert_eq!(commands(&[Word { text: "curl".to_owned(), quoted: true }]).len(), 0);
}

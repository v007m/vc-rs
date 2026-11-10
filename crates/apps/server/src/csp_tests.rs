use super::{FALLBACK, NO_HASHES, NOT_UTF8, header as sent, policy};

const HASH: &str = "'sha256-CbculNcflElcZFscHVVSn37ckaTi9ym23lQrblhKGW4='";

/// `index.html` as `pnpm -C web build` writes it.
const BUILT: &str = include_str!("../tests/fixtures/index.html");

fn page(meta_content: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"content-security-policy\" content=\"{meta_content}\"></head><body></body></html>"
    )
}

fn header(hashes: &str) -> String {
    format!(
        "default-src 'self'; script-src 'self' {hashes}; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
    )
}

fn of(html: &str) -> Result<String, &'static str> {
    policy(Some(html.as_bytes()))
}

#[test]
fn takes_the_script_hashes_of_the_built_page() {
    let html = page(&format!("default-src 'self'; script-src 'self' {HASH}"));
    assert_eq!(of(&html), Ok(header(HASH)));
}

#[test]
fn reads_the_page_the_web_build_writes() {
    let header = of(BUILT).unwrap();
    let hashes: Vec<&str> = header.split_ascii_whitespace().filter(|word| word.starts_with("'sha256-")).collect();
    assert_eq!(hashes.len(), 1, "{header}");
    assert!(BUILT.contains(hashes[0].trim_end_matches(';')), "{header}");
}

#[test]
fn reads_any_case_and_any_quoting() {
    let hash = HASH.trim_matches('\'');
    for meta in [
        format!("<META HTTP-EQUIV=\"Content-Security-Policy\" CONTENT=\"script-src '{hash}'\">"),
        format!("<meta http-equiv='content-security-policy' content=\"script-src '{hash}'\">"),
        format!("<meta content = \"script-src '{hash}'\" http-equiv=content-security-policy />"),
        format!("<meta data-x='a>b' http-equiv=content-security-policy content=\"script-src '{hash}'\">"),
    ] {
        assert_eq!(of(&meta), Ok(header(HASH)), "{meta}");
    }
}

#[test]
fn never_copies_unsafe_inline_or_a_foreign_host_into_the_header() {
    let html = page(&format!(
        "default-src *; script-src 'unsafe-inline' 'unsafe-eval' https://evil.example data: {HASH} 'sha256-a;b' 'sha256-<x>' 'nonce-abc'; connect-src *"
    ));
    let header = of(&html).unwrap();
    for refused in ["unsafe", "evil.example", "data:", "nonce", "<x>", "'sha256-a", "*", "connect-src"] {
        assert!(!header.contains(refused), "{refused} reached {header}");
    }
    assert!(header.contains(HASH));
}

#[test]
fn no_page_falls_back_and_a_page_without_hashes_is_an_error() {
    assert_eq!(policy(None), Ok(FALLBACK.to_owned()));
    assert_eq!(policy(Some(b"\xff<meta>")), Err(NOT_UTF8));
    for html in [
        "<html><head></head></html>".to_owned(),
        page("script-src 'self' 'unsafe-inline'"),
        page("default-src 'self'; script-src-elem 'self' 'sha256-AAAA'"),
        page("script-src 'sha256-' 'sha256-a b'"),
        "<meta http-equiv=\"content-security-policy\" content=\"script-src 'sha256-AAAA'".to_owned(),
    ] {
        assert_eq!(of(&html), Err(NO_HASHES), "{html}");
    }
}

#[test]
fn the_fallback_header_is_the_strict_policy_and_still_forbids_framing() {
    assert!(FALLBACK.contains("frame-ancestors 'none'"));
    assert_eq!(sent(None), (FALLBACK.parse().unwrap(), None));
    let (value, problem) = sent(Some(b"\xff<meta>"));
    assert_eq!((value.to_str().unwrap(), problem), (FALLBACK, Some(NOT_UTF8)));
}

#[test]
fn reads_only_the_csp_meta() {
    let html = format!(
        "<meta name=\"description\" content=\"script-src {HASH}\"><meta data-http-equiv=\"content-security-policy\" content=\"script-src 'sha256-BBBB'\">"
    );
    assert_eq!(of(&html), Err(NO_HASHES));
}

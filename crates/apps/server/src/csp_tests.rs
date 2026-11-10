use super::policy;

const HASH: &str = "'sha256-CbculNcflElcZFscHVVSn37ckaTi9ym23lQrblhKGW4='";
const FALLBACK: &str = "default-src 'self'; frame-ancestors 'none'";

fn page(meta_content: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"content-security-policy\" content=\"{meta_content}\"></head><body></body></html>"
    )
}

#[test]
fn takes_the_script_hashes_of_the_built_page() {
    let html = page(&format!("default-src 'self'; script-src 'self' {HASH}"));
    assert_eq!(
        policy(Some(&html)),
        format!(
            "default-src 'self'; script-src 'self' {HASH}; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
        )
    );
}

#[test]
fn never_copies_unsafe_inline_or_a_foreign_host_into_the_header() {
    let html = page(&format!(
        "default-src *; script-src 'unsafe-inline' 'unsafe-eval' https://evil.example data: {HASH} 'sha256-a;b' 'sha256-<x>' 'nonce-abc'; connect-src *"
    ));
    let header = policy(Some(&html));
    for refused in ["unsafe", "evil.example", "data:", "nonce", "<x>", "'sha256-a", "*", "connect-src"] {
        assert!(!header.contains(refused), "{refused} reached {header}");
    }
    assert!(header.contains(HASH));
}

#[test]
fn falls_back_without_a_page_a_meta_or_a_hash() {
    assert_eq!(policy(None), FALLBACK);
    assert_eq!(policy(Some("<html><head></head></html>")), FALLBACK);
    assert_eq!(policy(Some(&page("script-src 'self' 'unsafe-inline'"))), FALLBACK);
    assert_eq!(policy(Some(&page("default-src 'self'; script-src-elem 'self' 'sha256-AAAA'"))), FALLBACK);
    assert_eq!(policy(Some(&page("script-src 'sha256-' 'sha256-a b'"))), FALLBACK);
}

#[test]
fn reads_only_the_csp_meta() {
    let html = format!(
        "<meta name=\"description\" content=\"script-src {HASH}\"><meta data-http-equiv=\"content-security-policy\" content=\"script-src 'sha256-BBBB'\">"
    );
    assert_eq!(policy(Some(&html)), FALLBACK);
}

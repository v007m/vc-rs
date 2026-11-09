//! Curl's options, read into the request they describe. Each option is an entry of [`OPTIONS`];
//! one with no api form (output, timeouts, TLS, verbosity) is read past and left out.

use std::slice::Iter;

use super::shell::Word;

/// What one curl command sends, as its options give it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `-X`.
    pub method: Option<String>,
    /// The first URL.
    pub url: Option<String>,
    /// `-H`, as `name: value` split, in order.
    pub headers: Vec<(String, String)>,
    /// `-d` and its kin, URL-encoded where `--data-urlencode` asks; joined by `&` when sent.
    pub data: Vec<String>,
    /// `-F`, as `name=value`.
    pub form: Vec<String>,
    /// `--json`: the data is JSON, sent with JSON's content type.
    pub json: bool,
    /// `-G`: the data goes in the query.
    pub get: bool,
    /// `-I`: a HEAD request.
    pub head: bool,
    /// The credential options given (`-u`, `-b`); their values are never kept.
    pub credentials: Vec<&'static str>,
    /// What could not be read, without the values it held.
    pub notes: Vec<String>,
}

/// What an option does with its value, if it takes one.
#[derive(Clone, Copy)]
enum Takes {
    Value(fn(&mut Request, String)),
    Flag(fn(&mut Request)),
}

/// One option: its long names, its short names and what it does.
struct Opt {
    long: &'static [&'static str],
    short: &'static [char],
    takes: Takes,
}

const fn value(long: &'static [&'static str], short: &'static [char], apply: fn(&mut Request, String)) -> Opt {
    Opt { long, short, takes: Takes::Value(apply) }
}

const fn flag(long: &'static [&'static str], short: &'static [char], apply: fn(&mut Request)) -> Opt {
    Opt { long, short, takes: Takes::Flag(apply) }
}

/// The options read; any other is noted and taken as a flag.
const OPTIONS: &[Opt] = &[
    value(&["request"], &['X'], |request, method| request.method = Some(method)),
    value(&["url"], &[], Request::locate),
    value(&["header"], &['H'], |request, header| request.header(&header)),
    value(&["data", "data-ascii", "data-binary"], &['d'], Request::data),
    value(&["data-raw"], &[], |request, data| request.data.push(data)),
    value(&["data-urlencode"], &[], |request, data| request.data.push(url_encoded(&data))),
    value(&["json"], &[], |request, data| {
        request.json = true;
        request.data(data);
    }),
    value(&["form", "form-string"], &['F'], |request, field| request.form.push(field)),
    flag(&["get"], &['G'], |request| request.get = true),
    flag(&["head"], &['I'], |request| request.head = true),
    value(&["user"], &['u'], |request, _| request.credentials.push("-u")),
    value(&["cookie"], &['b'], |request, _| request.credentials.push("-b")),
    value(&["oauth2-bearer"], &[], |request, _| request.credentials.push("--oauth2-bearer")),
    value(&["proxy-user"], &['U'], |request, _| request.credentials.push("--proxy-user")),
    value(
        &[
            "output",
            "write-out",
            "max-time",
            "connect-timeout",
            "proxy",
            "cacert",
            "cert",
            "key",
            "retry",
            "user-agent",
        ],
        &['o', 'w', 'm', 'x', 'E', 'A'],
        |_, _| {},
    ),
    flag(
        &["compressed", "insecure", "location", "silent", "show-error", "verbose", "include", "fail", "globoff"],
        &['k', 'L', 's', 'S', 'v', 'i', 'f', 'g', 'N', '#'],
        |_| {},
    ),
    flag(&["http1.1", "http2", "http3", "no-buffer", "progress-bar", "digest", "basic", "ntlm"], &[], |_| {}),
];

/// The request one curl command's arguments describe.
pub fn request(args: &[Word]) -> Request {
    let mut request = Request::default();
    let mut rest = args.iter();
    while let Some(word) = rest.next() {
        let text = word.text.as_str();
        if let Some(long) = text.strip_prefix("--").filter(|long| !long.is_empty()) {
            let opt = OPTIONS.iter().find(|opt| opt.long.contains(&long));
            request.apply(opt, &format!("--{long}"), &mut rest);
        } else if let Some(shorts) = text.strip_prefix('-').filter(|shorts| !shorts.is_empty()) {
            request.shorts(shorts, &mut rest);
        } else {
            request.locate(word.text.clone());
        }
    }
    request
}

impl Request {
    /// A bundle of short options (`-sSL`); one that takes a value takes the rest of the word
    /// (`-XPOST`), or the next word.
    fn shorts(&mut self, shorts: &str, rest: &mut Iter<'_, Word>) {
        for (at, short) in shorts.char_indices() {
            let opt = OPTIONS.iter().find(|opt| opt.short.contains(&short));
            let attached = shorts.get(at + short.len_utf8()..).filter(|attached| !attached.is_empty());
            match (opt.map(|opt| opt.takes), attached) {
                (Some(Takes::Value(apply)), Some(attached)) => return apply(self, attached.to_owned()),
                _ => self.apply(opt, &format!("-{short}"), rest),
            }
        }
    }

    /// Does what `opt`, written as `name`, does, taking its value from `rest`.
    fn apply(&mut self, opt: Option<&Opt>, name: &str, rest: &mut Iter<'_, Word>) {
        match opt.map(|opt| opt.takes) {
            Some(Takes::Flag(apply)) => apply(self),
            Some(Takes::Value(apply)) => match rest.next() {
                Some(word) => apply(self, word.text.clone()),
                None => self.notes.push(format!("{name} has no value")),
            },
            None => self.notes.push(format!("{name} is not read")),
        }
    }

    fn locate(&mut self, url: String) {
        if self.url.is_none() {
            self.url = Some(url);
        } else {
            self.notes.push("a second URL is not read".to_owned());
        }
    }

    fn header(&mut self, header: &str) {
        match header.split_once(':') {
            Some((name, value)) => self.headers.push((name.trim().to_owned(), value.trim().to_owned())),
            None => self.notes.push("a -H that is not `name: value` is not read".to_owned()),
        }
    }

    /// `-d`: a value starting with `@` names a file, which is never read.
    fn data(&mut self, data: String) {
        match data.strip_prefix('@') {
            Some(file) => self.notes.push(format!("-d @{file}: the file it sends is not read")),
            None => self.data.push(data),
        }
    }
}

/// `--data-urlencode`: `content`, `=content` or `name=content`, with the content encoded.
fn url_encoded(data: &str) -> String {
    let (name, content) = data.split_once('=').unwrap_or(("", data));
    let content = percent_encoding::utf8_percent_encode(content, percent_encoding::NON_ALPHANUMERIC);
    if name.is_empty() { content.to_string() } else { format!("{name}={content}") }
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;

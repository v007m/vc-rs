//! A `.http` file (the VS Code REST Client and `JetBrains` HTTP Client format) read into its
//! requests: a `###` line starts one, `@name = value` defines a file variable, `#` and `//`
//! lines are comments. `{{…}}` is left as written for [`super::templates`].

/// What starts a request, and its title after it.
const SEPARATOR: &str = "###";

/// What starts a comment line.
const COMMENTS: [&str; 2] = ["#", "//"];

/// The comment tag that names a request.
const NAME_TAG: &str = "@name";

/// What starts a line of a response handler, which ends the body.
const HANDLERS: [&str; 3] = ["> ", ">> ", "<> "];

/// One request as the file writes it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Its `###` title, else its `# @name`.
    pub name: Option<String>,
    /// The method, when the request line gives one.
    pub method: Option<String>,
    /// The URL, query continuation lines appended.
    pub url: String,
    /// The headers as `name: value` split, in order.
    pub headers: Vec<(String, String)>,
    /// The body, when there is one.
    pub body: Option<String>,
    /// What could not be read, without the values it held.
    pub notes: Vec<String>,
}

/// A `.http` file's variables, in the order defined, and its requests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HttpFile {
    pub variables: Vec<(String, String)>,
    pub requests: Vec<Written>,
}

/// The variables and requests of `text`.
pub fn read(text: &str) -> HttpFile {
    let mut file = HttpFile::default();
    let mut title = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(next) = line.trim_start().strip_prefix(SEPARATOR) {
            file.block(title.take(), &lines);
            lines.clear();
            title = Some(next.trim_start_matches('#').trim()).filter(|title| !title.is_empty());
        } else {
            lines.push(line);
        }
    }
    file.block(title, &lines);
    file
}

impl HttpFile {
    /// Reads one block: variables and comments, then a request, if it has one.
    fn block(&mut self, title: Option<&str>, lines: &[&str]) {
        let mut name = title.map(str::to_owned);
        let mut rest = lines.iter().copied();
        while let Some(line) = rest.next().map(str::trim) {
            if let Some(comment) = comment(line) {
                name = name.or_else(|| tagged(comment));
            } else if let Some(variable) = variable(line) {
                self.variables.push(variable);
            } else if !line.is_empty() {
                let mut request = request_line(line);
                request.name = name;
                self.requests.push(request.rest(&rest.collect::<Vec<_>>()));
                return;
            }
        }
    }
}

impl Written {
    /// The query continuation lines, headers and body after the request line.
    fn rest(mut self, lines: &[&str]) -> Self {
        let mut at = 0;
        while let Some(line) = lines.get(at).map(|line| line.trim()).filter(|line| line.starts_with(['?', '&'])) {
            self.url.push_str(line);
            at += 1;
        }
        while let Some(line) = lines.get(at).map(|line| line.trim()).filter(|line| !line.is_empty()) {
            self.header(line);
            at += 1;
        }
        self.body(lines.get(at..).unwrap_or_default());
        self
    }

    fn header(&mut self, line: &str) {
        if comment(line).is_some() {
            return;
        }
        match line.split_once(':') {
            Some((name, value)) => self.headers.push((name.trim().to_owned(), value.trim().to_owned())),
            None => self.notes.push("a header line that is not `name: value` is not read".to_owned()),
        }
    }

    /// The body, up to a response handler; a body read from a file is not read.
    fn body(&mut self, lines: &[&str]) {
        let end = lines.iter().position(|line| HANDLERS.iter().any(|handler| line.trim_start().starts_with(handler)));
        if end.is_some() {
            self.notes.push("a response handler is not imported".to_owned());
        }
        let body = lines.get(..end.unwrap_or(lines.len())).unwrap_or_default().join("\n");
        let body = body.trim();
        if body.starts_with('<') {
            self.notes.push("a body read from a file is not read".to_owned());
        } else if !body.is_empty() {
            self.body = Some(body.to_owned());
        }
    }
}

/// `METHOD url HTTP/1.1`, `METHOD url` or `url`.
fn request_line(line: &str) -> Written {
    let without_version = line.rsplit_once(' ').filter(|(_, version)| version.starts_with("HTTP/"));
    let line = without_version.map_or(line, |(line, _)| line.trim_end());
    match line.split_once(' ') {
        Some((method, url)) if method.bytes().all(|byte| byte.is_ascii_uppercase()) => {
            Written { method: Some(method.to_owned()), url: url.trim().to_owned(), ..Written::default() }
        }
        _ => Written { url: line.to_owned(), ..Written::default() },
    }
}

/// The text of a comment line.
fn comment(line: &str) -> Option<&str> {
    COMMENTS.iter().find_map(|start| line.strip_prefix(start))
}

/// The request name a `# @name login` or `# @name = login` comment gives.
fn tagged(comment: &str) -> Option<String> {
    let name = comment.trim().strip_prefix(NAME_TAG)?.trim().trim_start_matches('=').trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// A file variable: `@name = value`.
fn variable(line: &str) -> Option<(String, String)> {
    let (name, value) = line.strip_prefix('@')?.split_once('=')?;
    let name = name.trim();
    let named = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    named.then(|| (name.to_owned(), value.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{HttpFile, Written, read};

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
    }

    #[test]
    fn blocks_give_variables_and_requests() {
        let text = "@host = https://a.test\n@id=7\n\n### Get a user\n# @name getUser\nGET {{host}}/users/{{id}} HTTP/1.1\n\
                    Accept: application/json\n\n###\n// @name = make\nPOST {{host}}/users\n  ?team=a\n  &role=b\n\
                    Content-Type: application/json\n\n{\n  \"name\": \"Ada\"\n}\n\n> {% client.global.set('x', 1) %}\n";
        let get = Written {
            name: Some("Get a user".to_owned()),
            method: Some("GET".to_owned()),
            url: "{{host}}/users/{{id}}".to_owned(),
            headers: owned(&[("Accept", "application/json")]),
            ..Written::default()
        };
        let post = Written {
            name: Some("make".to_owned()),
            method: Some("POST".to_owned()),
            url: "{{host}}/users?team=a&role=b".to_owned(),
            headers: owned(&[("Content-Type", "application/json")]),
            body: Some("{\n  \"name\": \"Ada\"\n}".to_owned()),
            notes: vec!["a response handler is not imported".to_owned()],
        };
        let expected =
            HttpFile { variables: owned(&[("host", "https://a.test"), ("id", "7")]), requests: vec![get, post] };
        assert_eq!(read(text), expected);
    }

    #[test]
    fn a_bare_url_is_a_request_and_a_file_body_is_not_read() {
        let read = read("https://a.test/ping\n\n###\nPUT /x\nbad header\n\n< ./body.json\n");
        let urls: Vec<_> =
            read.requests.iter().map(|request| (request.method.as_deref(), request.url.as_str())).collect();
        assert_eq!(urls, [(None, "https://a.test/ping"), (Some("PUT"), "/x")]);
        let notes = &read.requests.get(1).map(|request| request.notes.clone()).unwrap_or_default();
        assert_eq!(
            notes,
            &["a header line that is not `name: value` is not read", "a body read from a file is not read"]
        );
    }
}

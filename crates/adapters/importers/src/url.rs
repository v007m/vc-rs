//! URLs as an importer reads them: the origin a base-URL var holds, the path and the query.

use percent_encoding::percent_decode_str;

/// `scheme://host` and the rest after it, for a URL.
pub fn split_origin(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).map_or(url.len(), |at| scheme.len() + 3 + at);
    url.split_at_checked(end)
}

/// The path and the query of what follows an origin, without its `#fragment`.
pub fn path_and_query(rest: &str) -> (&str, &str) {
    let rest = rest.split('#').next().unwrap_or_default();
    rest.split_once('?').unwrap_or((rest, ""))
}

/// The `name=value` pairs of a query or a form body, with `+` and `%XX` decoded.
pub fn pairs(query: &str) -> impl Iterator<Item = (String, String)> + '_ {
    query.split('&').filter(|pair| !pair.is_empty()).map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (decoded(name), decoded(value))
    })
}

fn decoded(text: &str) -> String {
    percent_decode_str(&text.replace('+', " ")).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{pairs, path_and_query, split_origin};

    #[test]
    fn a_url_splits_at_the_end_of_its_origin() {
        assert_eq!(split_origin("https://a.test:8443/v1/x"), Some(("https://a.test:8443", "/v1/x")));
        assert_eq!(split_origin("https://a.test"), Some(("https://a.test", "")));
        assert_eq!(split_origin("https://a.test?q=1"), Some(("https://a.test", "?q=1")));
        assert_eq!(split_origin("/v1"), None);
    }

    #[test]
    fn the_rest_of_a_url_is_its_path_and_query_without_the_fragment() {
        assert_eq!(path_and_query("/a?q=1#top"), ("/a", "q=1"));
        assert_eq!(path_and_query("/a#b?c"), ("/a", ""));
        let read: Vec<_> = pairs("q=a+b%2Bc&&flag&e=%C3%A9").collect();
        let expected =
            [("q", "a b+c"), ("flag", ""), ("e", "é")].map(|(name, value)| (name.to_owned(), value.to_owned()));
        assert_eq!(read, expected);
    }
}

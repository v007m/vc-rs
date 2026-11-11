//! The noise in a recording: static assets (by extension and response type), `OPTIONS`
//! preflights, and calls to third-party sites. The project's own sites are those of the hosts
//! its base URLs render to, else the one the first entry calls; a site is a host's last two
//! labels, so a public suffix such as `co.uk` is not told apart. `--include-host` keeps a
//! host whatever its site, and `--keep-static` keeps static files.

use vc_interfaces::RecordingFilter;

use super::entry::Entry;
use crate::url::{host, path_and_query, split_origin};

/// The extensions of files a page loads rather than calls.
const STATIC_EXTENSIONS: [&str; 23] = [
    "js", "mjs", "css", "map", "html", "htm", "png", "jpg", "jpeg", "gif", "svg", "ico", "webp", "avif", "bmp", "woff",
    "woff2", "ttf", "otf", "eot", "mp4", "webm", "mp3",
];

/// Words in the response types of files a page loads rather than calls.
const STATIC_TYPES: [&str; 7] = ["text/css", "javascript", "text/html", "image/", "font/", "video/", "audio/"];

/// Why an entry is left out.
#[derive(Clone, Copy)]
enum Noise {
    Static,
    ThirdParty,
    Preflight,
}

/// The entries kept, a line saying what was filtered, and the third-party hosts left out, each
/// once in recorded order.
pub struct Kept {
    pub entries: Vec<Entry>,
    pub summary: String,
    pub third_party: Vec<String>,
}

/// What is not noise: the project's sites, the hosts kept whatever their site, and whether
/// static files are kept.
pub struct Rules {
    sites: Vec<String>,
    include: Vec<String>,
    keep_static: bool,
}

impl Rules {
    /// The rules `filter` asks for.
    pub fn new(filter: &RecordingFilter) -> Self {
        let lower = |hosts: &[String]| hosts.iter().map(|host| host.to_ascii_lowercase()).collect::<Vec<_>>();
        let sites = lower(&filter.hosts).iter().map(|host| site(host)).collect();
        Self { sites, include: lower(&filter.include_hosts), keep_static: filter.keep_static }
    }
}

/// The entries of `entries` that call the project's apis.
pub fn kept(entries: Vec<Entry>, rules: &Rules) -> Kept {
    let read = entries.len();
    let first = || entries.iter().find_map(|entry| host(&entry.request.url)).map(|host| site(&host));
    let homes = if rules.sites.is_empty() { first().into_iter().collect() } else { rules.sites.clone() };
    let mut counts = [0_usize; 3];
    let (mut kept, mut third_party) = (Vec::new(), Vec::<String>::new());
    for entry in entries {
        let noise = noise(&entry, &homes, rules);
        let host = host(&entry.request.url).unwrap_or_default();
        if matches!(noise, Some(Noise::ThirdParty)) && !third_party.contains(&host) {
            third_party.push(host);
        }
        match noise.and_then(|noise| counts.get_mut(noise as usize)) {
            Some(count) => *count += 1,
            None => kept.push(entry),
        }
    }
    let [assets, third, preflights] = counts;
    let summary = format!(
        "read {read} entries · filtered {assets} static, {third} third-party, {preflights} OPTIONS → {} requests",
        kept.len()
    );
    Kept { entries: kept, summary, third_party }
}

fn noise(entry: &Entry, homes: &[String], rules: &Rules) -> Option<Noise> {
    let url = &entry.request.url;
    if !(url.starts_with("http://") || url.starts_with("https://")) || (!rules.keep_static && asset(entry)) {
        return Some(Noise::Static);
    }
    if entry.request.method.eq_ignore_ascii_case("OPTIONS") {
        return Some(Noise::Preflight);
    }
    let host = host(url).unwrap_or_default();
    let home = rules.include.contains(&host) || homes.contains(&site(&host));
    (!home).then_some(Noise::ThirdParty)
}

/// Whether `entry` fetched a static file, told by its path's extension or its response type.
fn asset(entry: &Entry) -> bool {
    let path = split_origin(&entry.request.url).map_or("", |(_, rest)| path_and_query(rest).0);
    let last = path.rsplit('/').next().unwrap_or_default();
    let extension = last.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
    let typed = entry.response.content.mime_type.as_deref().map(str::to_ascii_lowercase).unwrap_or_default();
    extension.is_some_and(|extension| STATIC_EXTENSIONS.contains(&extension.as_str()))
        || STATIC_TYPES.iter().any(|word| typed.contains(word))
}

/// The site `host` belongs to: its last two labels, or the whole of an address or `localhost`.
fn site(host: &str) -> String {
    if host.contains(':') || !host.contains(|c: char| c.is_ascii_alphabetic()) {
        return host.to_owned();
    }
    let mut labels: Vec<_> = host.rsplit('.').take(2).collect();
    labels.reverse();
    labels.join(".")
}

#[cfg(test)]
mod tests {
    use super::{host, site};

    #[test]
    fn a_host_is_read_without_its_user_and_port_and_its_site_is_its_last_two_labels() {
        assert_eq!(host("https://u:p@API.Shop.test:8443/v1?q=1").as_deref(), Some("api.shop.test"));
        assert_eq!(host("http://[::1]:3000/x").as_deref(), Some("::1"));
        assert_eq!(host("data:image/png;base64,AA"), None);
        let sites: Vec<_> = ["api.shop.test", "shop.test", "localhost", "127.0.0.1", "::1"].map(site).into();
        assert_eq!(sites, ["shop.test", "shop.test", "localhost", "127.0.0.1", "::1"]);
    }
}

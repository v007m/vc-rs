use hyper_util::client::proxy::matcher::Matcher;

/// Proxy settings: which proxy, if any, to use for `http` and `https` targets, and which hosts
/// bypass it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct ProxyConfig {
    /// The proxy URL for `http://` targets.
    pub http: Option<String>,
    /// The proxy URL for `https://` targets (reached through a CONNECT tunnel).
    pub https: Option<String>,
    /// Hosts that bypass the proxy, in `NO_PROXY` syntax: comma-separated domains (subdomains
    /// match too), IPs or CIDR ranges, or `*` for every host.
    pub no_proxy: Option<String>,
}

impl ProxyConfig {
    /// Reads `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` through `lookup` (the environment, in
    /// production). The upper-case name wins over the lower-case one; empty values count as unset.
    #[must_use]
    pub fn from_env<F: Fn(&str) -> Option<String>>(lookup: F) -> Self {
        let read = |name: &str| {
            [name.to_owned(), name.to_lowercase()].iter().filter_map(|key| lookup(key)).find(|value| !value.is_empty())
        };
        Self { http: read("HTTP_PROXY"), https: read("HTTPS_PROXY"), no_proxy: read("NO_PROXY") }
    }

    /// The matcher the connector asks which proxy, if any, a target goes through.
    pub(crate) fn matcher(&self) -> Matcher {
        let mut builder = Matcher::builder();
        if let Some(http) = &self.http {
            builder = builder.http(http.as_str());
        }
        if let Some(https) = &self.https {
            builder = builder.https(https.as_str());
        }
        if let Some(no) = &self.no_proxy {
            builder = builder.no(no.as_str());
        }
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use http::Uri;

    use super::ProxyConfig;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_owned())
    }

    fn proxy_for(config: &ProxyConfig, target: &'static str) -> Option<String> {
        config.matcher().intercept(&Uri::from_static(target)).map(|found| found.uri().to_string())
    }

    #[test]
    fn reads_upper_case_names_before_lower_case_ones() {
        let lookup =
            env(&[("HTTPS_PROXY", "http://upper:1"), ("https_proxy", "http://lower:1"), ("http_proxy", "http://p:2")]);
        let config = ProxyConfig::from_env(lookup);
        assert_eq!(config.https.as_deref(), Some("http://upper:1"));
        assert_eq!(config.http.as_deref(), Some("http://p:2"), "lower case is the fallback");
        assert_eq!(config.no_proxy, None);
    }

    #[test]
    fn empty_values_count_as_unset() {
        let config = ProxyConfig::from_env(env(&[("HTTP_PROXY", ""), ("http_proxy", "http://p:2"), ("NO_PROXY", "")]));
        assert_eq!(config.http.as_deref(), Some("http://p:2"));
        assert_eq!(config.no_proxy, None);
    }

    #[test]
    fn no_environment_means_no_proxy() {
        let config = ProxyConfig::from_env(|_: &str| None);
        assert_eq!(config, ProxyConfig::default());
        assert_eq!(proxy_for(&config, "https://api.example.com/"), None);
    }

    #[test]
    fn targets_use_the_proxy_for_their_scheme_unless_no_proxy_lists_them() {
        let lookup = env(&[
            ("HTTP_PROXY", "http://plain:3128"),
            ("HTTPS_PROXY", "http://tunnel:3128"),
            ("NO_PROXY", "internal.test, 10.0.0.0/8"),
        ]);
        let config = ProxyConfig::from_env(lookup);
        assert_eq!(proxy_for(&config, "http://api.example.com/").as_deref(), Some("http://plain:3128/"));
        assert_eq!(proxy_for(&config, "https://api.example.com/").as_deref(), Some("http://tunnel:3128/"));
        assert_eq!(proxy_for(&config, "https://svc.internal.test/"), None, "subdomains match");
        assert_eq!(proxy_for(&config, "http://10.1.2.3/"), None, "CIDR ranges match");
    }
}

use async_trait::async_trait;
use vc_interfaces::{DraftApi, DraftGroup, ImportDraft, ImportError, ImportOptions, ImportSource, Importer};
use vc_model::Method;

/// The methods a line may start with: enough for a fake.
const METHODS: [(&str, Method); 3] = [("GET", Method::Get), ("POST", Method::Post), ("DELETE", Method::Delete)];

/// An importer of a made-up kind: one `METHOD /path` per line, each an api of the file `--id`
/// names, its ID the method and the path's words in camelCase (`GET /users` is `getUsers`).
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct FakeImporter;

impl FakeImporter {
    /// A file of this kind with two apis.
    pub const SAMPLE: &'static str = "GET /users\nPOST /users/{id}/roles\n";
}

#[async_trait]
impl Importer for FakeImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid =
            |reason: &str| ImportError::Invalid { location: source.location.clone(), reason: reason.to_owned() };
        let text = std::str::from_utf8(&source.bytes).map_err(|_| invalid("not UTF-8"))?;
        let apis: Option<Vec<DraftApi>> = text.lines().map(api).collect();
        let apis = apis.ok_or_else(|| invalid("a line is not METHOD /path"))?;
        if apis.is_empty() {
            return Err(invalid("no apis"));
        }
        let group = DraftGroup { file_id: opts.file_id.clone(), apis, ..DraftGroup::default() };
        Ok(ImportDraft { groups: vec![group], ..ImportDraft::default() })
    }
}

fn api(line: &str) -> Option<DraftApi> {
    let (written, path) = line.split_once(' ')?;
    let (name, method) = METHODS.iter().find(|(name, _)| *name == written)?;
    if !path.starts_with('/') {
        return None;
    }
    let words = path.split(['/', '{', '}']).filter(|word| !word.is_empty());
    let id = words.fold(name.to_ascii_lowercase(), |id, word| id + &capitalised(word));
    Some(DraftApi { id, method: *method, path: path.to_owned(), ..DraftApi::default() })
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{ImportOptions, ImportSource, Importer};
    use vc_model::Method;

    use super::FakeImporter;
    use crate::importer_suite;

    fn sample() -> ImportSource {
        ImportSource { location: "sample.txt".to_owned(), bytes: FakeImporter::SAMPLE.as_bytes().to_vec() }
    }

    #[tokio::test]
    async fn meets_the_importer_contract() {
        importer_suite(&FakeImporter, &sample()).await;
    }

    #[tokio::test]
    async fn names_apis_after_method_and_path() {
        let opts = ImportOptions { file_id: "users".to_owned(), ..ImportOptions::default() };
        let draft = FakeImporter.read(&sample(), &opts).await.unwrap_or_default();
        let group = draft.groups.first().cloned().unwrap_or_default();
        assert_eq!(group.file_id, "users");
        let apis: Vec<_> = group.apis.iter().map(|api| (api.id.as_str(), api.method, api.path.as_str())).collect();
        assert_eq!(
            apis,
            [("getUsers", Method::Get, "/users"), ("postUsersIdRoles", Method::Post, "/users/{id}/roles")]
        );
    }

    #[tokio::test]
    async fn refuses_a_path_without_a_slash() {
        let source = ImportSource { location: "x".to_owned(), bytes: b"GET users".to_vec() };
        assert!(FakeImporter.read(&source, &ImportOptions::default()).await.is_err());
    }
}

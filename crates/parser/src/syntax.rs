use vc_model::SourcePath;

/// The language a file is written in, which picks its reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Syntax {
    /// YAML 1.2: every definition file, and any file without a known suffix.
    Yaml,
    /// JSON (RFC 8259), read strictly by its own reader (D148).
    Json,
}

/// File-name suffixes with a syntax other than YAML.
const SUFFIXES: [(&str, Syntax); 1] = [(".json", Syntax::Json)];

impl Syntax {
    /// The syntax of the file at `path`, from its suffix: `.json` is JSON, anything else YAML.
    ///
    /// ```
    /// use vc_model::SourcePath;
    /// use vc_parser::Syntax;
    ///
    /// assert_eq!(Syntax::of(&SourcePath::new("schemas/user.json").unwrap()), Syntax::Json);
    /// assert_eq!(Syntax::of(&SourcePath::new("services/users/users.api.yaml").unwrap()), Syntax::Yaml);
    /// ```
    #[must_use]
    pub fn of(path: &SourcePath) -> Self {
        let name = path.file_name();
        SUFFIXES.iter().find(|(suffix, _)| name.ends_with(suffix)).map_or(Self::Yaml, |&(_, syntax)| syntax)
    }
}

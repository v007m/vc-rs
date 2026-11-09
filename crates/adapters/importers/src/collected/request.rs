//! A collection's request as written, in the form every collection importer drafts from: its
//! text still holds its `{{name}}` variables. No type here derives `Debug`, since the text may
//! hold secrets.

/// One request of a collection.
pub struct Call {
    /// Its name, which names the api.
    pub name: String,
    /// The method as written.
    pub method: String,
    /// The URL as written.
    pub url: String,
    /// The value of each `:name` path variable.
    pub path_values: Vec<(String, String)>,
    /// The enabled query parameters; none to read them from the URL.
    pub query: Option<Vec<(String, String)>>,
    /// The enabled headers.
    pub headers: Vec<(String, Option<String>)>,
    /// Where its auth sends a credential, which the file's auth sends instead.
    pub credential: Option<Credential>,
    /// Its body.
    pub body: Option<Payload>,
    /// What the user should know about it, such as a script that is not imported.
    pub notes: Vec<String>,
}

/// Where a request's auth sends its credential.
pub enum Credential {
    /// In the query parameter named.
    Query(String),
    /// In the header named.
    Header(String),
}

/// A request body as written.
pub enum Payload {
    /// Text, with the content type it has when no header names one.
    Raw { text: String, content_type: &'static str },
    /// The enabled fields of a form.
    Form(Vec<(String, String)>),
    /// A GraphQL query, and its variables as JSON text.
    GraphQl { query: String, variables: String },
    /// A body that is not imported, with the note that says so.
    Left(&'static str),
}

//! V2312: a literal `body.schema` that is not a valid JSON Schema (D307), compiled the way a run
//! compiles it (`vc_template::compile_schema`), so `vc check` reports it rather than every run's
//! V5101. A schema the program does not hold (a file not given, a templated path) is left to the
//! run.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Node;
use vc_template::{SchemaError, compile_schema};

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;

/// What a schema that breaks as a whole, not at one keyword, most often gets wrong.
const WHOLE: &str = "a `$ref` must point into the same file, and a `$schema` must name draft-04 to 2020-12";

/// Reports each `body.schema` whose schema does not compile.
pub struct BodySchemas;

impl Lint for BodySchemas {
    fn code(&self) -> Code {
        Code::V2312
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let nodes = cx
            .project
            .sources
            .iter()
            .flat_map(|source| link::schema_nodes(source.root).into_iter().map(move |node| (source, node)));
        nodes.filter_map(|(source, node)| self.broken(cx, source, node)).collect()
    }
}

impl BodySchemas {
    /// V2312 at the schema `node` when the schema it names is held and does not compile.
    fn broken(&self, cx: &LintCx<'_, '_>, source: &Source<'_>, node: &Node) -> Option<Diagnostic> {
        let key = link::schema_key(source, node);
        let Err(SchemaError::Invalid { at, reason, .. }) = compile_schema(&cx.program.schemas, &key) else {
            return None;
        };
        let inline = link::schema_is_inline(node);
        let place = if inline { "the schema written here".to_owned() } else { format!("`{key}`") };
        let help =
            if at.is_empty() { format!("fix {place} as a whole: {WHOLE}") } else { format!("fix `{at}` in {place}") };
        let message = format!("`body.schema` is not a valid JSON Schema: {reason}");
        let span = if inline { located(node, &at).span } else { node.span };
        Some(Diagnostic::new(self.code(), span, message).label("not a valid JSON Schema").help(help))
    }
}

/// The node the JSON pointer `at` names inside the inline schema at `schema`, or the deepest one
/// it reaches.
fn located<'n>(schema: &'n Node, at: &str) -> &'n Node {
    let mut node = schema;
    for segment in at.split('/').skip(1) {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        let item = || segment.parse().ok().and_then(|index: usize| node.items().get(index));
        let Some(next) = node.get(&segment).or_else(item) else { break };
        node = next;
    }
    node
}

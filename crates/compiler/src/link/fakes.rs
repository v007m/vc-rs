//! `${fake.…}` references and the config's named `generators` (D174): every name is a built-in
//! Faker generator or a named one (V2002), the literal arguments fit it (V2302, V2309 for a
//! `fromRegExp` pattern), and its pipes take the type it gives (V2304).

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{Namespace, Reference, Segment};
use vc_parser::Node;
use vc_template::{FakeCall, FakeError, FakeRegistry, PipeRegistry, ValueType};

use super::project;
use super::project::{Project, Source};
use super::uses;
use crate::schema;
use crate::suggest;

/// A named generator that lowers cleanly.
pub struct Named<'a> {
    /// Its `id`.
    pub id: &'a str,
    /// The built-in call it makes.
    pub call: FakeCall,
}

/// What a reference may name: the built-ins, and the named generators with their types.
struct Known {
    fakes: FakeRegistry,
    named: HashMap<String, ValueType>,
    pipes: PipeRegistry,
}

/// Checks the config's `generators`, then every `${fake.…}` outside CEL.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let fakes = FakeRegistry::builtin();
    let mut diagnostics = Diagnostics::new();
    let named = project.config().map(|config| lower(config, &fakes, &mut diagnostics)).unwrap_or_default();
    let named = named.into_iter().filter_map(|named| Some((named.id.to_owned(), fakes.get(named.call.name)?.output)));
    let known = Known { named: named.collect(), fakes, pipes: PipeRegistry::builtin() };
    for source in &project.sources {
        let fakes = uses::references(source, Namespace::Fake);
        diagnostics.extend(fakes.into_iter().filter_map(|reference| self::reference(source, reference, &known)));
    }
    diagnostics
}

/// The config's named generators that lower cleanly; the others are reported.
pub fn lower<'a>(config: &Source<'a>, fakes: &FakeRegistry, diagnostics: &mut Diagnostics) -> Vec<Named<'a>> {
    let modules = fakes.modules();
    let mut named = Vec::new();
    for node in project::list(config.root, "generators") {
        let (Some(id_node), Some(id)) = (node.get("id"), project::text(node, "id")) else { continue };
        if modules.contains(id) {
            let message = format!("`{id}` is a Faker module and cannot name a generator");
            let diagnostic = Diagnostic::new(Code::V2006, id_node.span, message).label("a Faker module");
            diagnostics.push(diagnostic.help(format!("pick another ID; `${{fake.{id}.…}}` is the built-in")));
            continue;
        }
        match generator(node, fakes) {
            Ok(call) => named.push(Named { id, call }),
            Err(diagnostic) => diagnostics.push(*diagnostic),
        }
    }
    named
}

/// The call one generator makes: it writes exactly one kind key, with settings that fit.
fn generator(node: &Node, fakes: &FakeRegistry) -> Result<FakeCall, Box<Diagnostic>> {
    let kinds = FakeRegistry::kinds();
    let written: Vec<_> = node.entries().iter().filter(|entry| kinds.contains(&entry.key.as_str())).collect();
    let [entry] = written.as_slice() else {
        let (code, message) = match written.len() {
            0 => (Code::V1005, "this generator has no kind".to_owned()),
            _ => (Code::V2302, "a generator has one kind, but this one has several".to_owned()),
        };
        let diagnostic = Diagnostic::new(code, node.span, message);
        return Err(Box::new(diagnostic.help(format!("write one of {}", suggest::quoted(&kinds)))));
    };
    fakes.named(&entry.key, &schema::to_json(&entry.value)).map_err(|error| {
        let id = project::text(node, "id").unwrap_or_default();
        Box::new(Diagnostic::new(error.code, entry.value.span, format!("generator `{id}` {}", error.message)))
    })
}

/// The dotted name a reference gives after `fake.`.
fn name(reference: &Reference) -> String {
    let keys = reference.path.segments.iter().filter_map(|segment| match segment {
        Segment::Key(key) => Some(key.as_str()),
        Segment::Index(_) | Segment::All => None,
    });
    keys.collect::<Vec<_>>().join(".")
}

/// The problem with one `${fake.…}`, if any.
fn reference(source: &Source<'_>, reference: &Reference, known: &Known) -> Option<Diagnostic> {
    let name = self::name(reference);
    let checked = match known.named.get(&name) {
        Some(output) if reference.args.is_empty() => Ok(*output),
        Some(_) => Err(FakeError::new(Code::V2302, "is a named generator and takes no arguments")),
        None => match known.fakes.check(&name, &reference.args) {
            Some(checked) => checked,
            None => return Some(unknown(source, reference, (&name, known))),
        },
    };
    match checked {
        Ok(output) => known.pipes.check_chain(&reference.pipes, output).err().filter(|it| it.code == Code::V2304),
        Err(error) => {
            Some(Box::new(Diagnostic::new(error.code, reference.span, format!("`fake.{name}` {}", error.message))))
        }
    }
    .map(|diagnostic| *diagnostic)
}

/// V2002 for a name that is no generator, with the close names and the full list.
fn unknown(source: &Source<'_>, reference: &Reference, (name, known): (&str, &Known)) -> Diagnostic {
    let message = format!("`fake.{name}` is not a generator");
    let diagnostic = Diagnostic::new(Code::V2002, reference.span, message).label("no generator has this name");
    let mut names = known.fakes.names();
    names.extend(known.named.keys().map(String::as_str));
    names.sort_unstable();
    let diagnostic = uses::offer(diagnostic, (source, reference, name), &close(name, &names));
    diagnostic.note(format!("the generators are {}", suggest::quoted(&names)))
}

/// The generators close to `name`: by its last word among its namespace's (`person.fullname` is
/// close to `person.fullName` only), or by the whole name when none of those is.
fn close<'a>(name: &str, names: &[&'a str]) -> Vec<&'a str> {
    let word_in = |ns: &str, generator: &'a str| generator.strip_prefix(ns)?.strip_prefix('.');
    if let Some((ns, word)) = name.rsplit_once('.') {
        let words = suggest::close(word, names.iter().filter_map(|generator| word_in(ns, generator)));
        let full = |word: &str| names.iter().copied().find(|generator| word_in(ns, generator) == Some(word));
        if !words.is_empty() {
            return words.into_iter().filter_map(full).collect();
        }
    }
    suggest::close(name, names.iter().copied())
}

#[cfg(test)]
mod tests {
    use super::close;

    const NAMES: [&str; 5] =
        ["internet.email", "person.firstName", "person.fullName", "person.jobTitle", "phone.number"];

    #[test]
    fn a_generator_is_close_by_its_last_word_in_its_namespace_or_else_by_its_whole_name() {
        assert_eq!(close("person.fullname", &NAMES), ["person.fullName"]);
        assert_eq!(close("person.fulName", &NAMES), ["person.fullName"]);
        assert_eq!(close("internet.emial", &NAMES), ["internet.email"]);
        assert_eq!(close("persn.fullName", &NAMES), ["person.fullName"]);
        assert_eq!(close("person.zzz", &NAMES), [] as [&str; 0]);
    }
}

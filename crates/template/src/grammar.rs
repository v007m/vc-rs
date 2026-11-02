//! The `${…}` grammar in winnow. It produces raw parts with byte ranges into the template
//! text; `parse.rs` turns them into model types with source spans. Every cut point carries an
//! `expected` context, and [`describe`] reports the innermost one.

use std::ops::Range;

use serde_json::{Number, Value};
use vc_model::Segment;
use winnow::ascii::{dec_int, digit1, space0};
use winnow::combinator::{alt, cut_err, delimited, not, opt, peek, preceded, repeat, separated, terminated};
use winnow::error::{ContextError, ParseError, StrContext, StrContextValue};
use winnow::token::{one_of, take_till, take_while};
use winnow::{LocatingSlice, ModalResult, Parser};

type In<'a> = LocatingSlice<&'a str>;

/// One piece of a template before spans are mapped.
pub enum RawPart {
    /// Literal text.
    Text(String),
    /// A `${…}` reference.
    Ref(RawRef),
}

/// A reference before its root is resolved.
pub struct RawRef {
    pub root: String,
    pub path: Vec<Segment>,
    pub args: Option<Vec<Value>>,
    pub pipes: Vec<RawPipe>,
    pub range: Range<usize>,
}

/// A pipe call with byte ranges.
pub struct RawPipe {
    pub name: String,
    pub name_range: Range<usize>,
    pub args: Vec<Value>,
    pub range: Range<usize>,
}

/// Parses a whole template; on failure, the error's offset is where it went wrong.
pub fn template(text: &str) -> Result<Vec<RawPart>, ParseError<In<'_>, ContextError>> {
    repeat(0.., part).parse(LocatingSlice::new(text))
}

/// What the innermost failing rule expected, such as ``expected `]` ``. Never empty.
pub fn describe(error: &ContextError) -> String {
    let innermost = error.context().find_map(|context| match context {
        StrContext::Expected(value) => Some(value.to_string()),
        _ => None,
    });
    innermost.map_or_else(|| "unexpected text".to_owned(), |value| format!("expected {value}"))
}

const fn expected(what: &'static str) -> StrContext {
    StrContext::Expected(StrContextValue::Description(what))
}

fn part(input: &mut In<'_>) -> ModalResult<RawPart> {
    alt((
        "$${".map(|_: &str| RawPart::Text("${".to_owned())),
        reference.map(RawPart::Ref),
        take_while(1.., |c| c != '$').map(|s: &str| RawPart::Text(s.to_owned())),
        terminated("$", not("{")).map(|s: &str| RawPart::Text(s.to_owned())),
    ))
    .parse_next(input)
}

fn reference(input: &mut In<'_>) -> ModalResult<RawRef> {
    let body = (space0, cut_err(ident).context(expected("a root such as `var`")), path, opt(args), pipes, space0);
    let closed = (body, cut_err("}").context(expected("`}` or `|`")));
    let (((_, root, path, args, pipes, _), _), range) = preceded("${", closed).with_span().parse_next(input)?;
    Ok(RawRef { root: root.to_owned(), path, args, pipes, range })
}

fn ident<'a>(input: &mut In<'a>) -> ModalResult<&'a str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(input)
}

fn path(input: &mut In<'_>) -> ModalResult<Vec<Segment>> {
    let key =
        preceded('.', cut_err(ident).context(expected("a key after `.`"))).map(|k: &str| Segment::Key(k.to_owned()));
    let inner = alt(('*'.value(Segment::All), dec_int.map(Segment::Index), quoted.map(Segment::Key)));
    let inner = cut_err(inner).context(expected("an index, `*` or a quoted key"));
    let index = delimited('[', inner, cut_err(']').context(expected("`]`")));
    repeat(0.., alt((key, index))).parse_next(input)
}

fn quoted(input: &mut In<'_>) -> ModalResult<String> {
    let double = delimited('"', take_till(0.., '"'), cut_err('"').context(expected("a closing `\"`")));
    let single = delimited('\'', take_till(0.., '\''), cut_err('\'').context(expected("a closing `'`")));
    alt((double, single)).map(str::to_owned).parse_next(input)
}

fn args(input: &mut In<'_>) -> ModalResult<Vec<Value>> {
    let item = delimited(space0, literal, space0);
    let list = separated(0.., item, ',');
    let close = cut_err(')').context(expected("a literal, `,` or `)`"));
    preceded('(', terminated(list, (space0, close))).parse_next(input)
}

fn literal(input: &mut In<'_>) -> ModalResult<Value> {
    alt((
        quoted.map(Value::String),
        "true".value(Value::Bool(true)),
        "false".value(Value::Bool(false)),
        "null".value(Value::Null),
        preceded(peek(one_of(|c: char| c == '-' || c.is_ascii_digit())), cut_err(number)).map(Value::Number),
    ))
    .parse_next(input)
}

/// A JSON number: `-12`, `0.5`, `1e3`, `2.5E-4`; no leading zeros.
fn number(input: &mut In<'_>) -> ModalResult<Number> {
    let digits = |what| cut_err(digit1).context(expected(what));
    let int = digits("a digit").verify(|d: &str| d == "0" || !d.starts_with('0'));
    let int = int.context(expected("a number without leading zeros"));
    let fraction = opt(('.', digits("a digit after `.`")));
    let exponent = opt((one_of(['e', 'E']), opt(one_of(['+', '-'])), digits("a digit in the exponent")));
    let text = (opt('-'), int, fraction, exponent).take();
    text.try_map(|s: &str| s.parse::<Number>()).context(expected("a number in range")).parse_next(input)
}

fn pipes(input: &mut In<'_>) -> ModalResult<Vec<RawPipe>> {
    let name = separated::<_, _, (), _, _, _, _>(1.., ident, '.').take().with_span();
    let call = (name, opt(args)).with_span().map(|(((name, name_range), args), range): (((&str, _), _), _)| RawPipe {
        name: name.to_owned(),
        name_range,
        args: args.unwrap_or_default(),
        range,
    });
    repeat(0.., preceded((space0, '|', space0), cut_err(call).context(expected("a pipe name")))).parse_next(input)
}

#![cfg(test)]

use serde_json::{Value, json};
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Part, Pos, ScalarSource, ScalarStyle, SourceFileId, SourceSpan, StringTable};
use vc_template::{ArgType, PipeArgs, PipeDef, PipeRegistry, PipeSignature, TemplateError, ValueType};

#[expect(clippy::unnecessary_wraps, reason = "a pipe has the `PipeFn` signature")]
fn mask(_: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    Ok(json!("***"))
}

/// Parses `${var.x | <pipes>}` on one plain line and returns its pipe calls.
fn calls(pipes: &str) -> Vec<vc_model::PipeCall> {
    let text = format!("${{var.x | {pipes}}}");
    let end = Pos::new(1, 1 + u32::try_from(text.len()).unwrap(), text.len());
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, end);
    pipes_of(&text, ScalarSource { span, style: ScalarStyle::Plain })
}

fn pipes_of(text: &str, source: ScalarSource) -> Vec<vc_model::PipeCall> {
    let Some(Part::Ref(reference)) = vc_template::parse(text, source, &mut StringTable::new()).unwrap().parts.pop()
    else {
        panic!("expected a trailing reference")
    };
    reference.pipes
}

fn chain(pipes: &str, input: ValueType) -> Result<ValueType, Box<Diagnostic>> {
    PipeRegistry::builtin().check_chain(&calls(pipes), input)
}

fn chain_err(pipes: &str, input: ValueType) -> Diagnostic {
    *chain(pipes, input).unwrap_err()
}

#[test]
fn builtin_pipes_chain_their_types() {
    assert_eq!(chain("trim | number", ValueType::Text).unwrap(), ValueType::Number);
    assert_eq!(chain("take(2) | random", ValueType::List).unwrap(), ValueType::Any);
    assert_eq!(chain("default('x') | upper", ValueType::Any).unwrap(), ValueType::Text);
    assert_eq!(chain("int | bool | string | json | jsonString", ValueType::Number).unwrap(), ValueType::Text);
    assert_eq!(chain("base64 | fromBase64 | lower | urlEncode", ValueType::Text).unwrap(), ValueType::Text);
    assert_eq!(chain("shuffle", ValueType::List).unwrap(), ValueType::List);
    assert_eq!(PipeRegistry::builtin().check_chain(&[], ValueType::Object).unwrap(), ValueType::Object);
}

#[test]
fn unknown_pipes_suggest_the_closest_name() {
    let diagnostic = chain_err("trim | uper", ValueType::Text);
    assert_eq!(diagnostic.code, Code::V2308);
    assert_eq!(diagnostic.message, "unknown pipe `uper`");
    assert_eq!(diagnostic.primary.span.byte_range(), 17..21);
    assert_eq!(diagnostic.help, ["did you mean `upper`?"]);
    let [fix] = &diagnostic.fixes[..] else { panic!("expected one fix") };
    assert!(!fix.certain);
    assert_eq!((fix.edits[0].new_text.as_str(), fix.edits[0].range.clone()), ("upper", 17..21));
}

#[test]
fn unknown_pipes_with_nothing_close_have_no_suggestion() {
    let diagnostic = chain_err("frobnicate", ValueType::Text);
    assert_eq!(diagnostic.code, Code::V2308);
    assert!(diagnostic.help.is_empty() && diagnostic.fixes.is_empty());
}

#[test]
fn wrong_argument_counts_and_types_are_v2308() {
    let count = chain_err("take", ValueType::List);
    assert_eq!((count.code, count.message.as_str()), (Code::V2308, "pipe `take` takes 1 argument(s), not 0"));
    assert_eq!(count.primary.span.byte_range(), 10..14);
    let extra = chain_err("trim(1)", ValueType::Text);
    assert_eq!(extra.message, "pipe `trim` takes 0 argument(s), not 1");
    for bad in ["take(-1)", "take('2')", "take(1.5)"] {
        let diagnostic = chain_err(bad, ValueType::List);
        assert_eq!(
            (diagnostic.code, diagnostic.message.as_str()),
            (Code::V2308, "argument 1 of pipe `take` must be a whole number ≥ 0")
        );
    }
    assert!(chain("take(0)", ValueType::List).is_ok());
}

#[test]
fn a_pipe_given_the_wrong_type_is_v2304() {
    let diagnostic = chain_err("trim | take(1)", ValueType::Text);
    assert_eq!((diagnostic.code, diagnostic.message.as_str()), (Code::V2304, "pipe `take` takes a list, not text"));
    assert_eq!(chain_err("number", ValueType::Object).code, Code::V2304);
    assert_eq!(chain_err("number", ValueType::List).code, Code::V2304);
}

#[test]
fn value_types_accept_themselves_any_and_scalars() {
    use ValueType::{Any, Bool, List, Number, Object, Scalar, Text};
    for scalar in [Text, Number, Bool, Any, Scalar] {
        assert!(Scalar.accepts(scalar), "{scalar:?}");
    }
    assert!(!Scalar.accepts(List) && !Scalar.accepts(Object));
    assert!(Any.accepts(Object) && Text.accepts(Any) && !Text.accepts(Number) && !Number.accepts(Text));
}

#[test]
fn plugins_register_pipes_under_their_prefix() {
    let mut registry = PipeRegistry::builtin();
    assert!(registry.get("acme.mask").is_none());
    let signature = PipeSignature::new(ValueType::Text, ValueType::Text, &[ArgType::Any]);
    registry.register("acme.mask", PipeDef::new(signature, mask)).unwrap();
    assert_eq!(registry.get("acme.mask").unwrap().signature.args, [ArgType::Any]);
    assert_eq!(registry.check_chain(&calls("acme.mask('*')"), ValueType::Text).unwrap(), ValueType::Text);
    assert_eq!(registry.check(&calls("acme.mask(1)")[0]).unwrap().input, ValueType::Text);
    assert_eq!(calls("acme.mask(1)")[0].args, [json!(1)]);
}

#[test]
fn registering_rejects_a_taken_name() {
    let mut registry = PipeRegistry::builtin();
    let signature = PipeDef::new(PipeSignature::new(ValueType::Text, ValueType::Text, &[]), mask);
    assert!(registry.register("acme.mask", signature).is_ok());
    let duplicate = registry.register("acme.mask", signature).unwrap_err();
    assert!(matches!(&duplicate, TemplateError::DuplicatePipe(name) if name == "acme.mask"));
    assert_eq!(duplicate.to_string(), "pipe `acme.mask` is already registered");
    assert!(registry.register("acme.text.mask", signature).is_ok());
}

#[test]
fn registering_rejects_names_without_a_prefix() {
    let mut registry = PipeRegistry::builtin();
    let signature = PipeDef::new(PipeSignature::new(ValueType::Text, ValueType::Text, &[]), mask);
    for name in ["mask", "trim", "acme.", ".mask", "acme..mask", ""] {
        let error = registry.register(name, signature).unwrap_err();
        assert!(matches!(&error, TemplateError::UnprefixedPipe(n) if n == name), "{name}");
    }
    let message = registry.register("mask", signature).unwrap_err().to_string();
    assert_eq!(message, "pipe `mask` needs a plugin prefix, such as `acme.mask`");
}

#[test]
fn types_describe_themselves_in_the_syntax_reference_words() {
    use ValueType::{Any, Bool, List, Number, Object, Scalar, Text};
    let words: Vec<String> = [Any, Scalar, Text, Number, Bool, List, Object].iter().map(ToString::to_string).collect();
    assert_eq!(
        words,
        ["any value", "text, a number or a boolean", "text", "a number", "a boolean", "a list", "an object"]
    );
    assert_eq!((ArgType::Any.to_string(), ArgType::Count.describe()), ("any literal".to_owned(), "a whole number ≥ 0"));
}

#[test]
fn approximate_spans_get_help_but_no_fix() {
    let text = "${var.x | uper}";
    let block = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::new(3, 1, 40));
    let pipes = pipes_of(text, ScalarSource { span: block, style: ScalarStyle::Block });
    let diagnostic = *PipeRegistry::builtin().check(&pipes[0]).unwrap_err();
    assert_eq!(diagnostic.primary.span, block);
    assert!(diagnostic.fixes.is_empty() && !diagnostic.help.is_empty());
}

#[test]
fn escaped_quoted_scalars_never_get_a_fix() {
    // `"\L\t${var.x | uper}"` decodes to 2 fewer bytes than its source: the length alone
    // cannot tell it from an unescaped scalar.
    let text = "\u{2028}\t${var.x | uper}";
    let line = |len: usize| Pos::new(1, 1 + u32::try_from(len).unwrap(), len);
    let source_len = text.len() + 2;
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, line(source_len));
    for style in [ScalarStyle::DoubleQuoted { escaped: true }, ScalarStyle::SingleQuoted { escaped: true }] {
        let diagnostic = *PipeRegistry::builtin().check(&pipes_of(text, ScalarSource { span, style })[0]).unwrap_err();
        assert_eq!(diagnostic.primary.span, span, "{style:?}");
        assert!(diagnostic.fixes.is_empty() && diagnostic.help == ["did you mean `upper`?"], "{style:?}");
    }
}

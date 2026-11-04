//! Literal arguments of pipes and generators: JSON numbers, `true`, `false` and single-quoted
//! text (D151). The grammar also reads `null` and double-quoted text, only to report them here.

use serde_json::Value;
use vc_diagnostics::{Diagnostic, Fix, TextEdit};

use crate::grammar::{ArgProblem, RawArg};
use crate::span_map::SpanMap;
use crate::{TemplateError, codes};

/// The values of `args`, or V2305 for the first one that is not an argument.
pub fn values(args: Vec<RawArg>, map: &SpanMap<'_>) -> Result<Vec<Value>, TemplateError> {
    args.into_iter().map(|arg| value(arg, map)).collect()
}

fn value(arg: RawArg, map: &SpanMap<'_>) -> Result<Value, TemplateError> {
    match arg.problem {
        None => Ok(arg.value),
        Some(problem) => Err(TemplateError::Syntax(Box::new(report(problem, &arg, map)))),
    }
}

fn report(problem: ArgProblem, arg: &RawArg, map: &SpanMap<'_>) -> Diagnostic {
    let span = map.span(arg.range.clone());
    let diagnostic = match problem {
        ArgProblem::Null => {
            Diagnostic::new(codes::TEMPLATE_SYNTAX, span, "invalid template: `null` is not an argument")
                .help("use single quotes for text, such as `''`")
        }
        ArgProblem::DoubleQuoted => {
            Diagnostic::new(codes::TEMPLATE_SYNTAX, span, "invalid template: text arguments take single quotes")
                .help("use single quotes")
        }
    };
    let text = arg.value.as_str().filter(|text| !text.contains(['\'', '\\']));
    match text {
        Some(text) if map.takes_single_quotes() => {
            let quoted = format!("'{text}'");
            diagnostic.fix(Fix::certain(format!("Write {quoted}"), vec![TextEdit::replace(span, quoted)]))
        }
        _ => diagnostic,
    }
}

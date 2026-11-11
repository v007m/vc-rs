//! The built-in pipes (LLD "Rendering", syntax reference "Converting types with pipes").

use crate::{ArgType, PipeDef, PipeSignature, ValueType, pipe_fns as run};

const NONE: &[ArgType] = &[];

const fn pipe(input: ValueType, output: ValueType, apply: run::PipeFn) -> PipeDef {
    PipeDef::new(PipeSignature::new(input, output, NONE), apply)
}

/// Each built-in pipe's name, signature and run-time function.
pub const BUILTIN: &[(&str, PipeDef)] = &[
    ("number", pipe(ValueType::TextOrNumber, ValueType::Number, run::number)),
    ("int", pipe(ValueType::TextOrNumber, ValueType::Number, run::int)),
    ("bool", pipe(ValueType::TextOrBool, ValueType::Bool, run::bool)),
    ("string", pipe(ValueType::Any, ValueType::Text, run::string)),
    ("json", pipe(ValueType::Text, ValueType::Any, run::json)),
    ("jsonString", pipe(ValueType::Any, ValueType::Text, run::json_string)),
    ("base64", pipe(ValueType::Text, ValueType::Text, run::base64)),
    ("fromBase64", pipe(ValueType::Text, ValueType::Text, run::from_base64)),
    ("lower", pipe(ValueType::Text, ValueType::Text, run::lower)),
    ("upper", pipe(ValueType::Text, ValueType::Text, run::upper)),
    ("trim", pipe(ValueType::Text, ValueType::Text, run::trim)),
    ("urlEncode", pipe(ValueType::Text, ValueType::Text, run::url_encode)),
    (
        "default",
        PipeDef::new(PipeSignature::new(ValueType::Any, ValueType::Any, &[ArgType::Any]), run::default)
            .taking_missing(),
    ),
    ("random", pipe(ValueType::List, ValueType::Any, run::random)),
    ("shuffle", pipe(ValueType::List, ValueType::List, run::shuffle)),
    ("take", PipeDef::new(PipeSignature::new(ValueType::List, ValueType::List, &[ArgType::Count]), run::take)),
];

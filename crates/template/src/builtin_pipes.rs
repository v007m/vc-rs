//! The built-in pipes (LLD "Rendering", syntax reference "Converting types with pipes").

use crate::{ArgType, PipeSignature, ValueType};

const NONE: &[ArgType] = &[];

/// Each built-in pipe's name and signature.
pub const BUILTIN: &[(&str, PipeSignature)] = &[
    ("number", PipeSignature::new(ValueType::Scalar, ValueType::Number, NONE)),
    ("int", PipeSignature::new(ValueType::Scalar, ValueType::Number, NONE)),
    ("bool", PipeSignature::new(ValueType::Scalar, ValueType::Bool, NONE)),
    ("string", PipeSignature::new(ValueType::Any, ValueType::Text, NONE)),
    ("json", PipeSignature::new(ValueType::Text, ValueType::Any, NONE)),
    ("jsonString", PipeSignature::new(ValueType::Any, ValueType::Text, NONE)),
    ("base64", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("fromBase64", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("lower", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("upper", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("trim", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("urlEncode", PipeSignature::new(ValueType::Text, ValueType::Text, NONE)),
    ("default", PipeSignature::new(ValueType::Any, ValueType::Any, &[ArgType::Any])),
    ("random", PipeSignature::new(ValueType::List, ValueType::Any, NONE)),
    ("shuffle", PipeSignature::new(ValueType::List, ValueType::List, NONE)),
    ("take", PipeSignature::new(ValueType::List, ValueType::List, &[ArgType::Count])),
];

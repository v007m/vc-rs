//! serde_json::Value <-> V8 conversion: serde_v8 (Direct) or V8's JSON.parse/stringify (JsonText).
use deno_core::{serde_v8, v8};
use hookbench::Marshal;
use serde_json::Value as Json;

pub fn to_v8<'s>(scope: &mut v8::PinScope<'s, '_>, v: &Json, how: Marshal) -> Result<v8::Local<'s, v8::Value>, String> {
    match how {
        Marshal::Direct => serde_v8::to_v8(scope, v).map_err(|e| e.to_string()),
        Marshal::JsonText => {
            let text = v8::String::new(scope, &v.to_string()).ok_or("string alloc failed")?;
            v8::json::parse(scope, text).ok_or_else(|| "JSON.parse failed".to_owned())
        }
    }
}

pub fn from_v8<'s>(scope: &mut v8::PinScope<'s, '_>, v: v8::Local<'s, v8::Value>, how: Marshal) -> Result<Json, String> {
    match how {
        Marshal::Direct => serde_v8::from_v8(scope, v).map_err(|e| e.to_string()),
        Marshal::JsonText => {
            let text = v8::json::stringify(scope, v).ok_or("JSON.stringify failed")?;
            serde_json::from_str(&text.to_rust_string_lossy(scope)).map_err(|e| e.to_string())
        }
    }
}

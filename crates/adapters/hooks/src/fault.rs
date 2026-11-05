//! Why a call failed, read out of `QuickJS` and then named as a [`HookError`].

use rquickjs::{CaughtError, Ctx, Error, Exception, Value};
use vc_interfaces::{HookCall, HookError, Limit};

/// A failure inside the runtime, before it is read.
#[derive(Debug)]
pub enum Fault {
    /// The value returned is not JSON; what it was.
    Bad(String),
    /// The module or export is not there.
    Missing,
    /// `QuickJS` or rquickjs reported an error, perhaps a thrown value.
    Js(Error),
}

impl From<Error> for Fault {
    fn from(error: Error) -> Self {
        Self::Js(error)
    }
}

/// A failure read out of the runtime, owning everything it needs once the context is left.
#[derive(Debug)]
pub enum Thrown {
    /// An `Error`: its name and message, and its stack.
    Error { message: String, stack: String },
    /// Some other thrown value, as text; `None` for `null` or `undefined`.
    Value(Option<String>),
    /// `QuickJS` could not allocate.
    Allocation,
    /// The value returned is not JSON.
    Bad(String),
    /// The module or export is not there.
    Missing,
    /// Anything else rquickjs reported.
    Other(String),
}

impl Thrown {
    /// `fault`, reading a thrown value out of `ctx`.
    pub fn read(ctx: &Ctx<'_>, fault: Fault) -> Self {
        match fault {
            Fault::Bad(what) => Self::Bad(what),
            Fault::Missing => Self::Missing,
            Fault::Js(error) => Self::caught(CaughtError::from_error(ctx, error)),
        }
    }

    fn caught(caught: CaughtError<'_>) -> Self {
        match caught {
            CaughtError::Error(Error::Allocation) => Self::Allocation,
            CaughtError::Error(Error::WouldBlock) => Self::Other("its promise never settled".to_owned()),
            CaughtError::Error(error) => Self::Other(error.to_string()),
            CaughtError::Exception(exception) => {
                Self::Error { message: describe(&exception), stack: exception.stack().unwrap_or_default() }
            }
            CaughtError::Value(value) => Self::Value(text(&value)),
        }
    }

    /// Whether it is a thrown `null` or `undefined`, which is what `QuickJS` throws when it
    /// cannot even allocate the out-of-memory error.
    pub const fn is_void(&self) -> bool {
        matches!(self, Self::Value(None))
    }
}

/// What the engine saw around a failed call.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    /// The interrupt handler stopped it.
    pub fired: bool,
    /// The runtime was at its memory limit afterwards.
    pub memory_full: bool,
}

/// `thrown` as a [`HookError`], its line mapped to TypeScript through `call`'s modules.
pub fn classify(thrown: Thrown, call: &HookCall, seen: Seen) -> HookError {
    let line = match &thrown {
        Thrown::Error { stack, .. } => ts_line(stack, call),
        _ => None,
    };
    if seen.fired {
        return HookError::Overran { limit: Limit::Time, line };
    }
    match thrown {
        Thrown::Allocation => HookError::Overran { limit: Limit::Memory, line },
        Thrown::Value(None) if seen.memory_full => HookError::Overran { limit: Limit::Memory, line },
        Thrown::Error { message, .. } if message.ends_with("out of memory") => {
            HookError::Overran { limit: Limit::Memory, line }
        }
        Thrown::Error { message, .. } | Thrown::Other(message) => HookError::Threw { message, line },
        Thrown::Value(text) => HookError::Threw { message: text.unwrap_or_else(|| "null".to_owned()), line },
        Thrown::Bad(what) => HookError::BadValue { what },
        Thrown::Missing => HookError::NotFound { hook: format!("{}#{}", call.hook.file, call.hook.export) },
    }
}

/// `Name: message`, as `String(error)` would show it.
fn describe(exception: &Exception<'_>) -> String {
    let message = exception.message().unwrap_or_default();
    match exception.as_object().get::<_, String>("name") {
        Ok(name) if !name.is_empty() => format!("{name}: {message}"),
        _ => message,
    }
}

/// A thrown value that is not an `Error`, as text; `None` for `null` or `undefined`.
fn text(value: &Value<'_>) -> Option<String> {
    if value.type_of().is_void() {
        return None;
    }
    let shown = value.as_string().and_then(|text| text.to_string().ok());
    Some(shown.unwrap_or_else(|| format!("a thrown {}", value.type_name())))
}

/// The TypeScript line of the first stack frame in one of `call`'s modules.
///
/// `QuickJS` writes each frame as `    at name (file:line:column)`.
fn ts_line(stack: &str, call: &HookCall) -> Option<u32> {
    stack.lines().find_map(|frame| {
        let at = frame.trim().strip_prefix("at ")?;
        let place = at.rsplit_once('(').map_or(at, |(_, place)| place).trim_end_matches(')');
        let mut parts = place.rsplitn(3, ':');
        let (_column, line, file) = (parts.next()?, parts.next()?, parts.next()?);
        call.modules.get(file)?.ts_line(line.parse().ok()?)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use vc_interfaces::{HookCall, HookError, Limit};
    use vc_model::{HookModule, HookModules, HookRef};

    use super::{Seen, Thrown, classify};

    fn call() -> HookCall {
        let module = |lines: Vec<u32>| HookModule { js: String::new(), lines };
        let modules = HookModules::new(BTreeMap::from([
            ("hooks/a.ts".to_owned(), module(vec![11, 12, 13])),
            ("hooks/lib/b.ts".to_owned(), module(vec![0, 21])),
        ]));
        HookCall::new(modules, HookRef { file: "hooks/a.ts".into(), export: "go".into() }, Value::Null)
    }

    const CALM: Seen = Seen { fired: false, memory_full: false };

    fn error(message: &str, stack: &str) -> Thrown {
        Thrown::Error { message: message.to_owned(), stack: stack.to_owned() }
    }

    #[test]
    fn maps_the_first_frame_in_a_hook_module_to_its_typescript_line() {
        let stack = "    at fail (native)\n    at helper (hooks/lib/b.ts:2:9)\n    at go (hooks/a.ts:3:1)\n";
        let threw = classify(error("Error: no", stack), &call(), CALM);
        assert_eq!(threw, HookError::Threw { message: "Error: no".into(), line: Some(21) });
        let unmapped = classify(error("Error: no", "    at hooks/lib/b.ts:1:1\n"), &call(), CALM);
        assert_eq!(unmapped, HookError::Threw { message: "Error: no".into(), line: None });
    }

    #[test]
    fn names_overruns_bad_values_and_missing_hooks() {
        let fired = Seen { fired: true, memory_full: false };
        let stopped = classify(error("InternalError: interrupted", "    at go (hooks/a.ts:2:3)"), &call(), fired);
        assert_eq!(stopped, HookError::Overran { limit: Limit::Time, line: Some(12) });
        let full = classify(error("InternalError: out of memory", ""), &call(), CALM);
        assert_eq!(full, HookError::Overran { limit: Limit::Memory, line: None });
        let null = Seen { fired: false, memory_full: true };
        assert!(matches!(classify(Thrown::Value(None), &call(), null), HookError::Overran { limit: Limit::Memory, .. }));
        assert!(matches!(classify(Thrown::Value(None), &call(), CALM), HookError::Threw { .. }));
        assert_eq!(classify(Thrown::Missing, &call(), CALM), HookError::NotFound { hook: "hooks/a.ts#go".into() });
        assert_eq!(classify(Thrown::Bad("x".into()), &call(), CALM), HookError::BadValue { what: "x".into() });
    }
}

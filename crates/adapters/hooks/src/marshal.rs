//! JSON into `QuickJS` values for a hook's input, and back out of what it returns.

use rquickjs::{Array, Ctx, IntoJs, Object, Type, Value};
use serde_json::{Map, Number, Value as Json};

use crate::fault::Fault;

/// How deep a returned value may nest; deeper (or cyclic) values are not JSON.
const MAX_DEPTH: usize = 64;

/// The largest integer a JavaScript number holds exactly: 2^53.
const EXACT: f64 = 9_007_199_254_740_992.0;

/// `json` as a JavaScript value: integers that fit in 32 bits stay integers.
pub fn to_js<'js>(ctx: &Ctx<'js>, json: &Json) -> rquickjs::Result<Value<'js>> {
    Ok(match json {
        Json::Null => Value::new_null(ctx.clone()),
        Json::Bool(flag) => Value::new_bool(ctx.clone(), *flag),
        Json::Number(number) => number.as_i64().and_then(|int| i32::try_from(int).ok()).map_or_else(
            || Value::new_float(ctx.clone(), number.as_f64().unwrap_or(f64::NAN)),
            |int| Value::new_int(ctx.clone(), int),
        ),
        Json::String(text) => text.as_str().into_js(ctx)?,
        Json::Array(items) => {
            let array = Array::new(ctx.clone())?;
            for (index, item) in items.iter().enumerate() {
                array.set(index, to_js(ctx, item)?)?;
            }
            array.into_value()
        }
        Json::Object(fields) => {
            let object = Object::new(ctx.clone())?;
            for (key, item) in fields {
                object.set(key.as_str(), to_js(ctx, item)?)?;
            }
            object.into_value()
        }
    })
}

/// A returned value as JSON: `undefined` is `null`, and an object drops its `undefined` fields.
///
/// # Errors
///
/// [`Fault::Bad`] for a function, symbol, big integer, non-finite number or a value nested too
/// deep; [`Fault::Js`] when reading a field throws.
pub fn to_json(value: &Value<'_>) -> Result<Json, Fault> {
    convert(value, 0)
}

fn convert(value: &Value<'_>, depth: usize) -> Result<Json, Fault> {
    if depth > MAX_DEPTH {
        return Err(Fault::Bad(format!("a value nested more than {MAX_DEPTH} deep, or cyclic")));
    }
    match value.type_of() {
        Type::Uninitialized | Type::Undefined | Type::Null => Ok(Json::Null),
        Type::Bool => Ok(Json::Bool(value.as_bool().unwrap_or_default())),
        Type::Int => Ok(Json::from(value.as_int().unwrap_or_default())),
        Type::Float => number(value.as_float().unwrap_or(f64::NAN)),
        Type::String => Ok(Json::String(value.as_string().map(rquickjs::String::to_string).transpose()?.unwrap_or_default())),
        Type::Array => array(value, depth),
        Type::Object => object(value, depth),
        other => Err(Fault::Bad(format!("a value of type `{}`", other.as_str()))),
    }
}

#[expect(clippy::cast_possible_truncation, reason = "the float is integral and within ±2^53, so the cast is exact")]
fn number(float: f64) -> Result<Json, Fault> {
    if float.fract() == 0.0 && float.abs() <= EXACT {
        return Ok(Json::from(float as i64));
    }
    Number::from_f64(float).map(Json::Number).ok_or_else(|| Fault::Bad("a number that is not finite".to_owned()))
}

fn array(value: &Value<'_>, depth: usize) -> Result<Json, Fault> {
    let Some(array) = value.as_array() else { return Ok(Json::Array(Vec::new())) };
    let mut items = Vec::with_capacity(array.len());
    for item in array.iter::<Value>() {
        items.push(convert(&item?, depth + 1)?);
    }
    Ok(Json::Array(items))
}

fn object(value: &Value<'_>, depth: usize) -> Result<Json, Fault> {
    let Some(object) = value.as_object() else { return Ok(Json::Object(Map::new())) };
    let mut fields = Map::new();
    for field in object.props::<String, Value>() {
        let (key, item) = field?;
        if !item.is_undefined() {
            fields.insert(key, convert(&item, depth + 1)?);
        }
    }
    Ok(Json::Object(fields))
}

#[cfg(test)]
mod tests {
    use rquickjs::{Context, Runtime};
    use serde_json::{Value as Json, json};

    use super::{to_js, to_json};
    use crate::fault::Fault;

    fn round_trip(js: &str) -> Result<Json, String> {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let value = ctx.eval::<rquickjs::Value, _>(js).unwrap();
            to_json(&value).map_err(|fault| match fault {
                Fault::Bad(what) => what,
                other => format!("{other:?}"),
            })
        })
    }

    #[test]
    fn reads_json_back_and_names_what_is_not_json() {
        assert_eq!(round_trip("({a: [1, 2.5, 'x', null, undefined], b: undefined, c: 2 ** 40})"), Ok(json!({"a": [1, 2.5, "x", null, null], "c": 1_099_511_627_776_i64})));
        assert_eq!(round_trip("undefined"), Ok(Json::Null));
        assert!(round_trip("(() => 1)").is_err_and(|what| what.contains("function")));
        assert!(round_trip("1 / 0").is_err_and(|what| what.contains("finite")));
        assert!(round_trip("{ const o = {}; o.o = o; o }").is_err_and(|what| what.contains("cyclic")));
    }

    #[test]
    fn writes_json_as_javascript() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let input = json!({"n": 2, "big": 5_000_000_000_i64, "list": [true, "s"], "none": null});
            let value = to_js(&ctx, &input).unwrap();
            assert_eq!(to_json(&value).ok(), Some(input));
        });
    }
}

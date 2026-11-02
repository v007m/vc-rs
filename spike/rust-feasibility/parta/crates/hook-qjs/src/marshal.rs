//! serde_json::Value <-> QuickJS value conversion. Two strategies are measured:
//! `Direct` walks the tree and builds JS objects; `JsonText` goes through
//! JSON.parse / JSON.stringify inside the engine.
use hookbench::Marshal;
use rquickjs::{Array, Ctx, Exception, IntoJs, Object, Type, Value};
use serde_json::{Map, Number, Value as Json};


pub fn to_js<'js>(ctx: &Ctx<'js>, v: &Json, how: Marshal) -> rquickjs::Result<Value<'js>> {
    match how {
        Marshal::Direct => json_to_js(ctx, v),
        Marshal::JsonText => ctx.json_parse(v.to_string()),
    }
}

pub fn from_js<'js>(ctx: &Ctx<'js>, v: &Value<'js>, how: Marshal) -> rquickjs::Result<Json> {
    match how {
        Marshal::Direct => js_to_json(ctx, v),
        Marshal::JsonText => {
            let text = ctx.json_stringify(v.clone())?.map(|s| s.to_string()).transpose()?;
            let text = text.unwrap_or_else(|| "null".to_owned());
            serde_json::from_str(&text).map_err(|e| Exception::throw_type(ctx, &e.to_string()))
        }
    }
}

pub fn json_to_js<'js>(ctx: &Ctx<'js>, v: &Json) -> rquickjs::Result<Value<'js>> {
    Ok(match v {
        Json::Null => Value::new_null(ctx.clone()),
        Json::Bool(b) => Value::new_bool(ctx.clone(), *b),
        Json::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => Value::new_int(ctx.clone(), i),
            None => Value::new_float(ctx.clone(), n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => s.as_str().into_js(ctx)?,
        Json::Array(items) => {
            let arr = Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() {
                arr.set(i, json_to_js(ctx, item)?)?;
            }
            arr.into_value()
        }
        Json::Object(map) => {
            let obj = Object::new(ctx.clone())?;
            for (k, item) in map {
                obj.set(k.as_str(), json_to_js(ctx, item)?)?;
            }
            obj.into_value()
        }
    })
}

pub fn js_to_json<'js>(ctx: &Ctx<'js>, v: &Value<'js>) -> rquickjs::Result<Json> {
    Ok(match v.type_of() {
        Type::Uninitialized | Type::Undefined | Type::Null => Json::Null,
        Type::Bool => Json::Bool(v.as_bool().unwrap_or(false)),
        Type::Int => Json::from(v.as_int().unwrap_or(0)),
        Type::Float => Number::from_f64(v.as_float().unwrap_or(f64::NAN)).map_or(Json::Null, Json::Number),
        Type::String => Json::String(v.as_string().map(rquickjs::String::to_string).transpose()?.unwrap_or_default()),
        Type::Array => {
            let arr = v.as_array().ok_or_else(|| Exception::throw_type(ctx, "array"))?;
            let items: rquickjs::Result<Vec<Json>> = arr.iter::<Value>().map(|x| js_to_json(ctx, &x?)).collect();
            Json::Array(items?)
        }
        Type::Object => {
            let obj = v.as_object().ok_or_else(|| Exception::throw_type(ctx, "object"))?;
            let mut map = Map::new();
            for prop in obj.props::<String, Value>() {
                let (k, x) = prop?;
                map.insert(k, js_to_json(ctx, &x)?);
            }
            Json::Object(map)
        }
        other => return Err(Exception::throw_type(ctx, &format!("hook returned non-JSON value: {other:?}"))),
    })
}

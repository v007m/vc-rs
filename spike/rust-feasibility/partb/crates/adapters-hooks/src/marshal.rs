//! Conversions between port DTOs, `serde_json` and `QuickJS` values.
use ir::{HeaderName, Method, UrlPath};
use ports::HttpRequest;
use rquickjs::{Array, Ctx, Exception, IntoJs, Object, Type, Value};
use serde_json::{Map, Number, Value as Json, json};

pub fn request_to_json(request: &HttpRequest) -> Json {
    let headers: Map<String, Json> =
        request.headers.iter().map(|(k, v)| (k.as_str().to_owned(), Json::String(v.clone()))).collect();
    json!({ "method": request.method, "path": request.path.as_str(), "headers": headers, "body": request.body })
}

pub fn request_from_json(value: &Json) -> Option<HttpRequest> {
    let method: Method = serde_json::from_value(value.get("method")?.clone()).ok()?;
    let path = UrlPath::new(value.get("path")?.as_str()?);
    let headers = value
        .get("headers")?
        .as_object()?
        .iter()
        .map(|(k, v)| v.as_str().map(|v| (HeaderName::new(k.as_str()), v.to_owned())))
        .collect::<Option<Vec<_>>>()?;
    let body = value.get("body").filter(|b| !b.is_null()).cloned();
    Some(HttpRequest { method, path, headers, body })
}

pub fn json_to_js<'js>(ctx: &Ctx<'js>, v: &Json) -> rquickjs::Result<Value<'js>> {
    Ok(match v {
        Json::Null => Value::new_null(ctx.clone()),
        Json::Bool(b) => Value::new_bool(ctx.clone(), *b),
        Json::Number(n) => n.as_i64().and_then(|i| i32::try_from(i).ok()).map_or_else(
            || Value::new_float(ctx.clone(), n.as_f64().unwrap_or(f64::NAN)),
            |i| Value::new_int(ctx.clone(), i),
        ),
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
        Type::Array => array_to_json(ctx, v)?,
        Type::Object => object_to_json(ctx, v)?,
        other => return Err(Exception::throw_type(ctx, &format!("hook returned non-JSON value: {other:?}"))),
    })
}

fn array_to_json<'js>(ctx: &Ctx<'js>, v: &Value<'js>) -> rquickjs::Result<Json> {
    let arr = v.as_array().ok_or_else(|| Exception::throw_type(ctx, "array"))?;
    arr.iter::<Value>().map(|x| js_to_json(ctx, &x?)).collect::<rquickjs::Result<Vec<_>>>().map(Json::Array)
}

fn object_to_json<'js>(ctx: &Ctx<'js>, v: &Value<'js>) -> rquickjs::Result<Json> {
    let obj = v.as_object().ok_or_else(|| Exception::throw_type(ctx, "object"))?;
    let mut map = Map::new();
    for prop in obj.props::<String, Value>() {
        let (k, x) = prop?;
        map.insert(k, js_to_json(ctx, &x)?);
    }
    Ok(Json::Object(map))
}

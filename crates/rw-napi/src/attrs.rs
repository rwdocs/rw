use std::sync::Arc;

use napi::bindgen_prelude::{JsObjectValue, Null, Object, ToNapiValue};
use napi::{Env, JsValue, Property, Result, Unknown, sys};
use serde_json::Value;

/// Output-only binding adapter retaining the selected page's canonical metadata.
/// JSON traversal is deferred until napi converts the response on the JS thread.
pub struct Attrs(pub(crate) Arc<rw_meta::Meta>);

impl ToNapiValue for Attrs {
    /// Converts attrs to a JavaScript object with ordinary data properties.
    ///
    /// # Errors
    /// Returns an error if a number cannot convert to `f64`, an array exceeds
    /// `u32::MAX` elements, or a Node-API operation fails.
    ///
    /// # Safety
    /// `env` must be valid on its JavaScript thread with an active handle scope.
    unsafe fn to_napi_value(env: sys::napi_env, val: Self) -> Result<sys::napi_value> {
        let env = Env::from_raw(env);
        Ok(object(&env, val.0.attrs.iter())?.raw())
    }
}

fn object<'env, 'value>(
    env: &'env Env,
    entries: impl Iterator<Item = (&'value String, &'value Value)>,
) -> Result<Object<'env>> {
    let mut object = Object::new(env)?;
    for (key, value) in entries {
        let converted_value = json_value(env, value)?;
        // A JS string key preserves embedded NUL. Defining a data property also
        // avoids invoking Object.prototype's __proto__ setter.
        let property = Property::new()
            .with_name(env, key.as_str())?
            .with_napi_value(env, converted_value)?;
        object.define_properties(&[property])?;
    }
    Ok(object)
}

fn json_value<'env>(env: &'env Env, value: &Value) -> Result<Unknown<'env>> {
    match value {
        Value::Null => Null.into_unknown(env),
        Value::Bool(value) => value.into_unknown(env),
        Value::String(value) => value.as_str().into_unknown(env),
        Value::Number(value) => {
            // All JSON numbers are JS Numbers, including u64 (with ordinary JS
            // precision loss), never napi's serde-json unsigned string fallback.
            let number = value
                .as_f64()
                .ok_or_else(|| napi::Error::from_reason("Invalid JSON number in page attrs"))?;
            number.into_unknown(env)
        }
        Value::Array(values) => {
            let len = u32::try_from(values.len())
                .map_err(|_| napi::Error::from_reason("Page attrs array is too large"))?;
            let mut array = env.create_array(len)?;
            for (index, value) in (0..len).zip(values) {
                array.set(index, json_value(env, value)?)?;
            }
            array.into_unknown(env)
        }
        Value::Object(values) => object(env, values.iter())?.into_unknown(env),
    }
}

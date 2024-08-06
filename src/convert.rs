//! This module contains functions for converting from JSON to CBOR and vice-versa.

use ciborium::Value as CborValue;
use serde_json::Value as JsonValue;

/// Convert a `JsonValue` to a `CborValue`
pub fn json_to_cbor(value: JsonValue) -> anyhow::Result<CborValue> {
    let value = match value {
        JsonValue::Number(inner) => {
            if inner.is_u64() {
                inner.as_u64().unwrap().into()
            } else if inner.is_i64() {
                inner.as_i64().unwrap().into()
            } else if inner.is_f64() {
                inner.as_f64().unwrap().into()
            } else {
                anyhow::bail!("'{inner:?}' did not fit into u64/i64/f64 categories")
            }
        }
        JsonValue::Null => CborValue::Null,
        JsonValue::Bool(inner) => inner.into(),
        JsonValue::String(inner) => inner.into(),
        JsonValue::Object(inner) => inner
            .into_iter()
            .map(|(k, v)| Ok((k.into(), json_to_cbor(v)?)))
            .collect::<anyhow::Result<Vec<(CborValue, CborValue)>>>()?
            .into(),
        JsonValue::Array(inner) => inner
            .into_iter()
            .map(json_to_cbor)
            .collect::<anyhow::Result<Vec<CborValue>>>()?
            .into(),
    };

    Ok(value)
}

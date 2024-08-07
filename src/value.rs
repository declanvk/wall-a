//! The Value enum, a loosely typed way of representing any valid JSON value.

pub mod merge;
mod serde;

use std::fmt::Debug;
use std::vec::Vec;

/// Represents any valid JSON value.
#[derive(
    Default,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    minicbor::Encode,
    minicbor::Decode,
    minicbor::CborLen,
)]
pub enum Value {
    /// Represents a JSON null value.
    #[default]
    #[n(0)]
    Null,

    /// Represents a JSON boolean.
    #[n(1)]
    Bool(#[n(0)] bool),

    /// Represents a JSON number, whether integer or floating point.
    #[n(2)]
    Number(#[n(0)] String),
    /// Represents a JSON string.
    #[n(3)]
    String(#[n(0)] String),

    /// Represents a JSON array.
    #[n(4)]
    Array(#[n(0)] Vec<Value>),

    /// Represents a JSON object.
    #[n(5)]
    Object(#[n(0)] Vec<(String, Value)>),
}

impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(inner) => Value::Bool(inner),
            serde_json::Value::Number(inner) => Value::Number(inner.to_string()),
            serde_json::Value::String(inner) => Value::String(inner),
            serde_json::Value::Array(inner) => {
                Value::Array(inner.into_iter().map(From::from).collect())
            }
            serde_json::Value::Object(inner) => Value::Object(
                inner
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<Vec<_>>(),
            ),
        }
    }
}

impl TryFrom<Value> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let value = match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(inner) => serde_json::Value::Bool(inner),
            Value::Number(inner) => serde_json::Value::Number(inner.parse()?),
            Value::String(inner) => serde_json::Value::String(inner),
            Value::Array(inner) => serde_json::Value::Array(
                inner
                    .iter()
                    .cloned()
                    .map(serde_json::Value::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            Value::Object(inner) => serde_json::Value::Object(
                inner
                    .iter()
                    .cloned()
                    .map(|(key, value)| Ok((key, serde_json::Value::try_from(value)?)))
                    .collect::<Result<_, _>>()?,
            ),
        };

        Ok(value)
    }
}

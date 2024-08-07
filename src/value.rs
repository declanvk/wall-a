//! The Value enum, a loosely typed way of representing any valid JSON value.

pub mod merge;
mod serde;

use std::borrow::Cow;
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
pub enum Value<'a> {
    /// Represents a JSON null value.
    #[default]
    #[n(0)]
    Null,

    /// Represents a JSON boolean.
    #[n(1)]
    Bool(#[n(0)] bool),

    /// Represents a JSON number, whether integer or floating point.
    #[n(2)]
    Number(#[b(0)] Cow<'a, str>),
    /// Represents a JSON string.
    #[n(3)]
    String(#[b(0)] Cow<'a, str>),

    /// Represents a JSON array.
    #[n(4)]
    Array(#[b(0)] Cow<'a, [Value<'a>]>),

    /// Represents a JSON object.
    #[n(5)]
    Object(#[b(0)] Cow<'a, [(Cow<'a, str>, Value<'a>)]>),
}

impl<'a> Value<'a> {
    /// TODO
    pub fn into_owned(self) -> Value<'static> {
        match self {
            Value::Null => Value::Null,
            Value::Bool(inner) => Value::Bool(inner),
            Value::Number(inner) => Value::Number(inner.into_owned().into()),
            Value::String(inner) => Value::String(inner.into_owned().into()),
            Value::Array(inner) => Value::Array(
                inner
                    .into_owned()
                    .into_iter()
                    .map(Value::into_owned)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Value::Object(inner) => Value::Object(
                inner
                    .into_owned()
                    .into_iter()
                    .map(|(k, v)| (k.into_owned().into(), v.into_owned()))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }
}

impl From<serde_json::Value> for Value<'static> {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(inner) => Value::Bool(inner),
            serde_json::Value::Number(inner) => Value::Number(inner.to_string().into()),
            serde_json::Value::String(inner) => Value::String(inner.into()),
            serde_json::Value::Array(inner) => {
                Value::Array(inner.into_iter().map(From::from).collect())
            }
            serde_json::Value::Object(inner) => Value::Object(Cow::Owned(
                inner
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect::<Vec<_>>(),
            )),
        }
    }
}

impl<'a> TryFrom<Value<'a>> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let value = match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(inner) => serde_json::Value::Bool(inner),
            Value::Number(inner) => serde_json::Value::Number(inner.parse()?),
            Value::String(inner) => serde_json::Value::String(inner.into_owned()),
            Value::Array(inner) => serde_json::Value::Array(
                inner
                    .into_owned()
                    .into_iter()
                    .map(serde_json::Value::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            Value::Object(inner) => serde_json::Value::Object(
                inner
                    .into_owned()
                    .into_iter()
                    .map(|(key, value)| Ok((key.into(), serde_json::Value::try_from(value)?)))
                    .collect::<Result<_, _>>()?,
            ),
        };

        Ok(value)
    }
}

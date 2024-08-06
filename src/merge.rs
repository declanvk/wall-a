//! This module contains functions for merge JSON and CBOR data with some configuration

use std::str::FromStr;

use indexmap::IndexSet;
use itertools::{EitherOrBoth, Itertools};
use serde_json::{Map, Value as JsonValue};

/// This struct defines how JSON & CBOR values are merged
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct MergeSettings {
    /// This field controls how arrays are merged
    pub array_behavior: ArrayBehavior,
    /// This field controls how null values are merged
    pub null_behavior: NullBehavior,
}

impl MergeSettings {
    /// Merge two JSON values together, favouring the second value as the more
    /// recent.
    ///
    /// The basic merge rule is:
    ///  - If both values are objects, then it takes the union of fields. For any
    ///    key that is present in both objects, it merges the associated values
    ///  - If both values are arrays, then the [`ArrayBehavior`] controls the merge
    ///    behavior
    ///  - If the second value is `null`, then the [`NullBehavior`] controls the
    ///    merge behavior
    ///  - Otherwise, the second value is used
    pub fn merge_json(self, accum: JsonValue, value: JsonValue) -> JsonValue {
        match (accum, value) {
            // For all shared keys, merge
            (JsonValue::Object(accum), JsonValue::Object(mut value)) => {
                let mut result =
                    Map::<String, JsonValue>::with_capacity(accum.len().max(value.len()));

                for (key, accum) in accum {
                    if value.contains_key(key.as_str()) {
                        // For key present in both objects, merge the values
                        let value = value.remove(key.as_str()).unwrap();
                        result.insert((*key).into(), self.merge_json(accum, value));
                    } else {
                        // For key present only in the accum value, add directly
                        result.insert((*key).into(), accum);
                    }
                }

                // For keys present only in the new value, add directly
                for (key, value) in value {
                    result.insert((*key).into(), value);
                }

                JsonValue::Object(result)
            }
            (JsonValue::Array(mut accum), JsonValue::Array(mut value)) => {
                let values: Vec<_> = match self.array_behavior {
                    // Append newer value to accumulator value
                    ArrayBehavior::Concat => {
                        accum.append(&mut value);
                        accum
                    }
                    // for all positions which have both, merge them. Otherwise append
                    ArrayBehavior::Merge => accum
                        .into_iter()
                        .zip_longest(value)
                        .map(|pair| match pair {
                            EitherOrBoth::Both(accum, value) => self.merge_json(accum, value),
                            EitherOrBoth::Left(value) | EitherOrBoth::Right(value) => value,
                        })
                        .collect(),
                    // Move all values through a hashset to get the unique set
                    ArrayBehavior::Union => accum
                        .into_iter()
                        .chain(value)
                        .collect::<IndexSet<_>>()
                        .into_iter()
                        .collect(),
                    // Take newer value
                    ArrayBehavior::Replace => value,
                };

                JsonValue::Array(values)
            }
            (accum, JsonValue::Null) => match self.null_behavior {
                NullBehavior::Ignore => accum,
                NullBehavior::Merge => JsonValue::Null,
            },
            // Fallback rule always takes newer value
            (_, value) => value,
        }
    }
}

/// This enum describes how array values are merged
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrayBehavior {
    /// Concatenate arrays
    #[default]
    Concat,
    /// Merge array items together, matched by index
    Merge,
    /// Union arrays, skipping items that already exist
    Union,
    /// Replace all array items
    Replace,
}

impl FromStr for ArrayBehavior {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "concat" => Self::Concat,
            "merge" => Self::Merge,
            "union" => Self::Union,
            "replace" => Self::Replace,
            x => anyhow::bail!("'{x}' is an unknown option for merging array values"),
        })
    }
}

/// This enum conrtols how `null` values are merged
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NullBehavior {
    ///  The content's null value properties will be merged
    #[default]
    Merge,
    ///  The content's null value properties will be ignored during merging
    Ignore,
}

impl FromStr for NullBehavior {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "merge" => Self::Merge,
            "ignore" => Self::Ignore,
            x => anyhow::bail!("'{x}' is an unknown option for merging null values"),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn default_settings_merge_json_basic() {
        let settings = MergeSettings::default();
        assert_eq!(settings.null_behavior, NullBehavior::Merge);

        assert_eq!(
            settings.merge_json(json!("hello"), json!("world")),
            json!("world")
        );
        assert_eq!(settings.merge_json(json!("hello"), json!(100)), json!(100));
        assert_eq!(
            settings.merge_json(json!("hello"), JsonValue::Null),
            JsonValue::Null
        );
        assert_eq!(settings.merge_json(json!(100), json!(100.0)), json!(100.0));
    }

    #[test]
    fn ignore_null_behavior() {
        let mut settings = MergeSettings::default();
        settings.null_behavior = NullBehavior::Ignore;

        assert_eq!(
            settings.merge_json(json!("hello"), JsonValue::Null),
            json!("hello")
        );
        assert_eq!(
            settings.merge_json(JsonValue::Null, JsonValue::Null),
            JsonValue::Null
        );
        assert_eq!(
            settings.merge_json(JsonValue::Null, json!("goodbye")),
            json!("goodbye")
        );
    }

    #[test]
    fn default_settings_merge_json_arrays() {
        let settings = MergeSettings::default();
        assert_eq!(settings.array_behavior, ArrayBehavior::Concat);

        assert_eq!(settings.merge_json(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge_json(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "c", "d", "e", "a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge_json(
                json!([
                    {"hello":"sun"}, {"goodbye":"moon"}
                ]),
                json!([
                    {"goodbye":"moon"},{"hello":"sun"}
                ])
            ),
            json!([
                {"hello":"sun"}, {"goodbye":"moon"}, {"goodbye":"moon"},{"hello":"sun"}
            ])
        );
    }

    #[test]
    fn union_array_behavior() {
        let mut settings = MergeSettings::default();
        settings.array_behavior = ArrayBehavior::Union;

        assert_eq!(settings.merge_json(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge_json(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "c", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge_json(
                json!([
                    {"hello":"sun"}, {"goodbye":"moon"}
                ]),
                json!([
                    {"goodbye":"moon"},{"hello":"sun"}
                ])
            ),
            json!([{"hello":"sun"}, {"goodbye":"moon"}])
        );
    }

    #[test]
    fn merge_array_behavior() {
        let mut settings = MergeSettings::default();
        settings.array_behavior = ArrayBehavior::Merge;

        assert_eq!(settings.merge_json(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge_json(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge_json(
                json!([
                    {"goodbye":"sun"}, {"hello":"moon", "something": "else"}
                ]),
                json!([
                    {"goodbye":"moon"},{"hello":"sun", "or": "this"}
                ])
            ),
            json!([
                {"goodbye":"moon"}, {"hello":"sun", "something": "else", "or": "this"}
            ])
        );
    }

    #[test]
    fn replace_array_behavior() {
        let mut settings = MergeSettings::default();
        settings.array_behavior = ArrayBehavior::Replace;

        assert_eq!(settings.merge_json(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge_json(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge_json(
                json!([
                    {"hello":"sun"}, {"goodbye":"moon", "something": "else"}
                ]),
                json!([
                    {"goodbye":"moon"},{"hello":"sun", "or": "this"}
                ])
            ),
            json!([
                {"goodbye":"moon"},{"hello":"sun", "or": "this"}
            ])
        );
    }

    #[test]
    fn default_settings_merge_json_objects() {
        let settings = MergeSettings::default();

        assert_eq!(settings.merge_json(json!({}), json!({})), json!({}));
        assert_eq!(
            settings.merge_json(
                json!({
                    "hello": "sun",
                    "goodbye": "moon",
                    "other": 100,
                }),
                json!({
                    "hello": "moon",
                    "goodbye": "sun",
                    "also-other": 100,
                })
            ),
            json!({
                "hello": "moon",
                "goodbye": "sun",
                "other": 100,
                "also-other": 100,
            })
        );
        assert_eq!(
            settings.merge_json(
                json!({
                    "hello": "sun",
                    "goodbye": "moon",
                    "other": 100,
                }),
                json!({})
            ),
            json!({
                "hello": "sun",
                "goodbye": "moon",
                "other": 100,
            })
        );
        assert_eq!(
            settings.merge_json(
                json!({
                    "hello": "sun",
                    "goodbye": {
                        "type": "planet",
                        "name": "pluto",
                    },
                }),
                json!({
                    "hello": "moon",
                    "goodbye": {
                        "type": "dwarf planet",
                    },
                })
            ),
            json!({
                "hello": "moon",
                "goodbye": {
                    "type": "dwarf planet",
                    "name": "pluto",
                },
            })
        );
    }
}

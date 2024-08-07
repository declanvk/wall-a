//! This module contains functions for merge JSON and CBOR data with some configuration

use std::{borrow::Cow, collections::HashMap, str::FromStr};

use indexmap::IndexSet;
use itertools::{EitherOrBoth, Itertools};

use super::Value;

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
    pub fn merge<'a>(self, accum: Value<'a>, value: Value<'a>) -> Value<'a> {
        match (accum, value) {
            // For all shared keys, merge
            (Value::Object(mut accum), Value::Object(value)) => {
                let mut keys = HashMap::with_capacity(accum.len().max(value.len()));

                for (accum_index, (key, _)) in accum.iter().enumerate() {
                    keys.insert(key.clone(), EitherOrBoth::Left(accum_index));
                }

                for (value_index, (key, _)) in value.iter().enumerate() {
                    keys.entry(key.clone())
                        .and_modify(|e| {
                            let accum_index = e.clone().left().unwrap();
                            *e = EitherOrBoth::Both(accum_index, value_index);
                        })
                        .or_insert(EitherOrBoth::Right(value_index));
                }

                for indices in keys.into_values() {
                    match indices {
                        EitherOrBoth::Both(accum_index, value_index) => {
                            let new_value = self
                                .merge(accum[accum_index].1.clone(), value[value_index].1.clone());
                            accum.to_mut()[accum_index].1 = new_value;
                        }
                        EitherOrBoth::Left(_) => {
                            // do nothing in this case, since accum already has the key
                        }
                        EitherOrBoth::Right(value_index) => {
                            // need to extend accum in this case since there is key from value that is
                            // not already present
                            accum.to_mut().push(value[value_index].clone())
                        }
                    }
                }

                Value::Object(accum)
            }
            (Value::Array(mut accum), Value::Array(value)) => {
                let values: Cow<'_, _> = match self.array_behavior {
                    // Append newer value to accumulator value
                    ArrayBehavior::Concat => {
                        accum.to_mut().extend(value.iter().cloned());
                        accum
                    }
                    // for all positions which have both, merge them. Otherwise append
                    ArrayBehavior::Merge => accum
                        .into_iter()
                        .zip_longest(value.into_iter())
                        .map(|pair| match pair {
                            EitherOrBoth::Both(accum, value) => {
                                self.merge(accum.clone(), value.clone())
                            }
                            EitherOrBoth::Left(value) | EitherOrBoth::Right(value) => value.clone(),
                        })
                        .collect(),
                    // Move all values through a hashset to get the unique set
                    ArrayBehavior::Union => accum
                        .into_iter()
                        .chain(value.into_iter())
                        .collect::<IndexSet<_>>()
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .into(),
                    // Take newer value
                    ArrayBehavior::Replace => value,
                };

                Value::Array(values)
            }
            (accum, Value::Null) => match self.null_behavior {
                NullBehavior::Ignore => accum,
                NullBehavior::Merge => Value::Null,
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
    macro_rules! json {
        ($input:tt) => {
            crate::value::Value::from(::serde_json::json!($input))
        };
    }

    use super::*;

    #[test]
    fn default_settings_merge_basic() {
        let settings = MergeSettings::default();
        assert_eq!(settings.null_behavior, NullBehavior::Merge);

        assert_eq!(
            settings.merge(json!("hello"), json!("world")),
            json!("world")
        );
        assert_eq!(settings.merge(json!("hello"), json!(100)), json!(100));
        assert_eq!(settings.merge(json!("hello"), Value::Null), Value::Null);
        assert_eq!(settings.merge(json!(100), json!(100.0)), json!(100.0));
    }

    #[test]
    fn ignore_null_behavior() {
        let mut settings = MergeSettings::default();
        settings.null_behavior = NullBehavior::Ignore;

        assert_eq!(settings.merge(json!("hello"), Value::Null), json!("hello"));
        assert_eq!(settings.merge(Value::Null, Value::Null), Value::Null);
        assert_eq!(
            settings.merge(Value::Null, json!("goodbye")),
            json!("goodbye")
        );
    }

    #[test]
    fn default_settings_merge_arrays() {
        let settings = MergeSettings::default();
        assert_eq!(settings.array_behavior, ArrayBehavior::Concat);

        assert_eq!(settings.merge(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "c", "d", "e", "a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge(
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

        assert_eq!(settings.merge(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "c", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge(
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

        assert_eq!(settings.merge(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge(
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

        assert_eq!(settings.merge(json!([]), json!([])), json!([]));
        assert_eq!(
            settings.merge(
                json!(["a", "b", "c", "d", "e"]),
                json!(["a", "b", "d", "e", "f"])
            ),
            json!(["a", "b", "d", "e", "f"])
        );
        assert_eq!(
            settings.merge(
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
    fn default_settings_merge_objects() {
        let settings = MergeSettings::default();

        assert_eq!(settings.merge(json!({}), json!({})), json!({}));
        assert_eq!(
            settings.merge(
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
            settings.merge(
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
            settings.merge(
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

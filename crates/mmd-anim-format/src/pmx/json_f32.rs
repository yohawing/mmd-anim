use std::fmt;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
    ser::SerializeSeq,
};

const NAN_TOKEN: &str = "NaN";
const INFINITY_TOKEN: &str = "Infinity";
const NEG_INFINITY_TOKEN: &str = "-Infinity";

#[derive(Clone, Copy)]
struct JsonF32(f32);

impl Serialize for JsonF32 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.0.is_nan() {
            serializer.serialize_str(NAN_TOKEN)
        } else if self.0 == f32::INFINITY {
            serializer.serialize_str(INFINITY_TOKEN)
        } else if self.0 == f32::NEG_INFINITY {
            serializer.serialize_str(NEG_INFINITY_TOKEN)
        } else {
            serializer.serialize_f32(self.0)
        }
    }
}

impl<'de> Deserialize<'de> for JsonF32 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonF32Visitor)
    }
}

struct JsonF32Visitor;

impl<'de> Visitor<'de> for JsonF32Visitor {
    type Value = JsonF32;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a finite f32 number or one of \"NaN\", \"Infinity\", \"-Infinity\"")
    }

    fn visit_f32<E>(self, value: f32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JsonF32(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JsonF32(value as f32))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JsonF32(value as f32))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(JsonF32(value as f32))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_json_f32(value).map(JsonF32)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&value)
    }
}

fn parse_json_f32<E>(value: &str) -> Result<f32, E>
where
    E: de::Error,
{
    match value {
        NAN_TOKEN => Ok(f32::NAN),
        INFINITY_TOKEN => Ok(f32::INFINITY),
        NEG_INFINITY_TOKEN => Ok(f32::NEG_INFINITY),
        _ => {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| E::custom(format!("invalid f32 string {value:?}")))?;
            if parsed.is_finite() {
                Ok(parsed)
            } else {
                Err(E::custom(format!(
                    "non-finite f32 string {value:?} must use one of \"NaN\", \"Infinity\", \"-Infinity\""
                )))
            }
        }
    }
}

pub fn serialize<S>(value: &f32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    JsonF32(*value).serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    JsonF32::deserialize(deserializer).map(|value| value.0)
}

fn serialize_slice<S>(values: &[f32], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(&JsonF32(*value))?;
    }
    seq.end()
}

fn deserialize_vec<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<JsonF32>::deserialize(deserializer)
        .map(|values| values.into_iter().map(|value| value.0).collect())
}

fn deserialize_array<'de, D, const N: usize>(deserializer: D) -> Result<[f32; N], D::Error>
where
    D: Deserializer<'de>,
{
    struct ArrayVisitor<const N: usize>;

    impl<'de, const N: usize> Visitor<'de> for ArrayVisitor<N> {
        type Value = [f32; N];

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "an array of {N} f32 values")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = [0.0; N];
            for (index, value) in values.iter_mut().enumerate() {
                *value = seq
                    .next_element::<JsonF32>()?
                    .ok_or_else(|| de::Error::invalid_length(index, &self))?
                    .0;
            }
            Ok(values)
        }
    }

    deserializer.deserialize_seq(ArrayVisitor::<N>)
}

struct JsonF32Slice<'a>(&'a [f32]);

impl Serialize for JsonF32Slice<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_slice(self.0, serializer)
    }
}

pub mod vec_f32 {
    use super::*;

    pub fn serialize<S>(values: &[f32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_slice(values, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_vec(deserializer)
    }
}

pub mod nested_vec {
    use super::*;

    pub fn serialize<S>(values: &[Vec<f32>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(values.len()))?;
        for values in values {
            seq.serialize_element(&JsonF32Slice(values))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<f32>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<Vec<JsonF32>>::deserialize(deserializer).map(|values| {
            values
                .into_iter()
                .map(|values| values.into_iter().map(|value| value.0).collect())
                .collect()
        })
    }
}

pub mod array3 {
    use super::*;

    pub fn serialize<S>(values: &[f32; 3], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_slice(values, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[f32; 3], D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_array(deserializer)
    }
}

pub mod array4 {
    use super::*;

    pub fn serialize<S>(values: &[f32; 4], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_slice(values, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[f32; 4], D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_array(deserializer)
    }
}

pub mod option_array3 {
    use super::*;

    pub fn serialize<S>(values: &Option<[f32; 3]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match values {
            Some(values) => serializer.serialize_some(&JsonF32Slice(values)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<[f32; 3]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptionArray3Visitor;

        impl<'de> Visitor<'de> for OptionArray3Visitor {
            type Value = Option<[f32; 3]>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("null or an array of 3 f32 values")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                super::array3::deserialize(deserializer).map(Some)
            }
        }

        deserializer.deserialize_option(OptionArray3Visitor)
    }
}

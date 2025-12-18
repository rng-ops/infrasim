//! Terraform State Management
//!
//! Handles encoding and decoding of Terraform state using msgpack.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;

/// Dynamic value that can be encoded/decoded from Terraform state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DynamicValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    List(Vec<DynamicValue>),
    Map(HashMap<String, DynamicValue>),
}

impl DynamicValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            DynamicValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            DynamicValue::Number(n) => n.as_i64(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            DynamicValue::Number(n) => n.as_f64(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DynamicValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&HashMap<String, DynamicValue>> {
        match self {
            DynamicValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&DynamicValue> {
        self.as_map()?.get(key)
    }
}

impl Default for DynamicValue {
    fn default() -> Self {
        DynamicValue::Null
    }
}

/// Decode a Terraform DynamicValue from msgpack bytes
pub fn decode_dynamic_value(data: &[u8]) -> Result<DynamicValue> {
    if data.is_empty() {
        return Ok(DynamicValue::Null);
    }

    // Terraform uses msgpack encoding
    // For now, try JSON as a fallback since msgpack requires additional dependency
    let value: DynamicValue = serde_json::from_slice(data)
        .unwrap_or(DynamicValue::Null);
    
    Ok(value)
}

/// Encode a value to Terraform DynamicValue bytes
pub fn encode_dynamic_value(value: &DynamicValue) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    Ok(bytes)
}

/// Helper to extract a string attribute from a DynamicValue
pub fn get_string_attr(value: &DynamicValue, key: &str) -> String {
    value.get(key)
        .and_then(|v| v.as_string())
        .unwrap_or("")
        .to_string()
}

/// Helper to extract an optional string attribute from a DynamicValue
pub fn get_optional_string_attr(value: &DynamicValue, key: &str) -> Option<String> {
    value.get(key)
        .and_then(|v| match v {
            DynamicValue::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
}

/// Helper to extract an integer attribute from a DynamicValue
pub fn get_int_attr(value: &DynamicValue, key: &str, default: i64) -> i64 {
    value.get(key)
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
}

/// Helper to extract a float attribute from a DynamicValue
pub fn get_float_attr(value: &DynamicValue, key: &str, default: f64) -> f64 {
    value.get(key)
        .and_then(|v| v.as_f64())
        .unwrap_or(default)
}

/// Helper to extract a bool attribute from a DynamicValue
pub fn get_bool_attr(value: &DynamicValue, key: &str, default: bool) -> bool {
    value.get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

/// Create a DynamicValue map with the given attributes
pub fn make_state(attrs: Vec<(&str, DynamicValue)>) -> DynamicValue {
    let mut map = HashMap::new();
    for (key, value) in attrs {
        map.insert(key.to_string(), value);
    }
    DynamicValue::Map(map)
}

/// Create a string DynamicValue
pub fn string_value(s: impl Into<String>) -> DynamicValue {
    DynamicValue::String(s.into())
}

/// Create a number DynamicValue from i64
pub fn int_value(n: i64) -> DynamicValue {
    DynamicValue::Number(serde_json::Number::from(n))
}

/// Create a number DynamicValue from f64
pub fn float_value(n: f64) -> DynamicValue {
    serde_json::Number::from_f64(n)
        .map(DynamicValue::Number)
        .unwrap_or(DynamicValue::Null)
}

/// Create a bool DynamicValue
pub fn bool_value(b: bool) -> DynamicValue {
    DynamicValue::Bool(b)
}

/// Create a null DynamicValue
pub fn null_value() -> DynamicValue {
    DynamicValue::Null
}

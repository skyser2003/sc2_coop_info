use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn null() -> Self {
        Value::Null
    }

    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i128().and_then(|v| u64::try_from(v).ok())
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn get_key(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn object_mut(&mut self) -> Option<&mut BTreeMap<String, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn insert_object_entry(&mut self, key: impl Into<String>, value: Value) {
        if let Value::Object(map) = self {
            map.insert(key.into(), value);
        }
    }

    pub fn first_field_value(&self) -> Option<&Value> {
        match self {
            Value::Object(map) => map.values().next(),
            _ => None,
        }
    }
}

impl From<JsonValue> for Value {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => Value::Null,
            JsonValue::Bool(v) => Value::Bool(v),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i128::from(i))
                } else if let Some(u) = n.as_u64() {
                    Value::Int(i128::from(u))
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            JsonValue::String(s) => Value::String(s),
            JsonValue::Array(items) => Value::Array(items.into_iter().map(Value::from).collect()),
            JsonValue::Object(obj) => {
                Value::Object(obj.into_iter().map(|(k, v)| (k, Value::from(v))).collect())
            }
        }
    }
}

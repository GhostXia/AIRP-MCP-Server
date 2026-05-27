//! State tracking models

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Live state for a character/session
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LiveState {
    pub values: HashMap<String, StateValue>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// State value can be various types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateValue {
    Number { value: f64, max: Option<f64> },
    Text(String),
    Object(serde_json::Value),
}

/// State schema for UI rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSchema {
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub key: String,
    pub label: String,
    pub field_type: FieldType,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Number,
    Bar,
    Text,
    List,
}

impl LiveState {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            updated_at: chrono::Utc::now(),
        }
    }
    
    /// Update state with delta
    pub fn update(&mut self, delta: serde_json::Value) {
        if let serde_json::Value::Object(map) = delta {
            for (key, value) in map {
                // Try to parse as number with max
                if let Some(obj) = value.as_object() {
                    if let (Some(val), max) = (
                        obj.get("value").and_then(|v| v.as_f64()),
                        obj.get("max").and_then(|v| v.as_f64()),
                    ) {
                        self.values.insert(key, StateValue::Number { value: val, max });
                        continue;
                    }
                }
                
                // Try as number directly
                if let Some(num) = value.as_f64() {
                    self.values.insert(key, StateValue::Number { value: num, max: None });
                    continue;
                }
                
                // Try as string
                if let Some(text) = value.as_str() {
                    self.values.insert(key, StateValue::Text(text.to_string()));
                    continue;
                }
                
                // Store as object
                self.values.insert(key, StateValue::Object(value));
            }
        }
        self.updated_at = chrono::Utc::now();
    }
    
    /// Format for system prompt injection
    pub fn format_for_prompt(&self, schema: Option<&StateSchema>) -> String {
        let mut lines = vec!["[Current State]".to_string()];
        
        if let Some(schema) = schema {
            for field in &schema.fields {
                if let Some(value) = self.values.get(&field.key) {
                    let formatted = match value {
                        StateValue::Number { value, max } => {
                            if let Some(max) = max {
                                format!("- {}: {:.0}/{:.0}", field.label, value, max)
                            } else {
                                format!("- {}: {}", field.label, value)
                            }
                        }
                        StateValue::Text(t) => format!("- {}: {}", field.label, t),
                        StateValue::Object(o) => format!("- {}: {}", field.label, o),
                    };
                    lines.push(formatted);
                }
            }
        } else {
            // No schema, just dump all values
            for (key, value) in &self.values {
                let formatted = match value {
                    StateValue::Number { value, max } => {
                        if let Some(max) = max {
                            format!("- {}: {:.0}/{:.0}", key, value, max)
                        } else {
                            format!("- {}: {}", key, value)
                        }
                    }
                    StateValue::Text(t) => format!("- {}: {}", key, t),
                    StateValue::Object(o) => format!("- {}: {}", key, o),
                };
                lines.push(formatted);
            }
        }
        
        lines.push("\nUpdate your state in <state>{...}</state> tags when values change.".to_string());
        lines.join("\n")
    }
}

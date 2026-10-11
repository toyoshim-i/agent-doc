use crate::mcp::types::{HandlerResult, ToolResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    String,
    Integer { min: Option<i64>, max: Option<i64> },
    Number,
    Boolean,
    Enum(Vec<String>),
    StringArray,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolParam {
    pub name: String,
    pub param_type: ParamType,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

pub type ToolHandler = Arc<dyn Fn(serde_json::Value) -> HandlerResult<ToolResult> + Send + Sync>;

#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub params: Vec<ToolParam>,
    pub see_page: Option<String>,
    pub examples: Vec<serde_json::Value>,
    pub handler: ToolHandler,
    pub skip_validation: bool,
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("params", &self.params)
            .field("see_page", &self.see_page)
            .field("examples", &self.examples)
            .finish()
    }
}

impl Tool {
    pub fn builder(name: impl Into<String>, description: impl Into<String>) -> ToolBuilder {
        ToolBuilder::new(name, description)
    }

    pub fn input_schema(&self) -> serde_json::Value {
        let mut properties = serde_json::Map::new();
        let mut required_fields = Vec::new();

        for p in &self.params {
            let mut prop = serde_json::Map::new();
            match &p.param_type {
                ParamType::String => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("string".to_string()),
                    );
                }
                ParamType::Integer { min, max } => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("integer".to_string()),
                    );
                    if let Some(min_val) = min {
                        prop.insert("minimum".to_string(), serde_json::json!(*min_val));
                    }
                    if let Some(max_val) = max {
                        prop.insert("maximum".to_string(), serde_json::json!(*max_val));
                    }
                }
                ParamType::Number => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("number".to_string()),
                    );
                }
                ParamType::Boolean => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("boolean".to_string()),
                    );
                }
                ParamType::Enum(variants) => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("string".to_string()),
                    );
                    prop.insert("enum".to_string(), serde_json::json!(variants));
                }
                ParamType::StringArray => {
                    prop.insert(
                        "type".to_string(),
                        serde_json::Value::String("array".to_string()),
                    );
                    prop.insert("items".to_string(), serde_json::json!({ "type": "string" }));
                }
            }
            if !p.description.is_empty() {
                prop.insert(
                    "description".to_string(),
                    serde_json::Value::String(p.description.clone()),
                );
            }
            properties.insert(p.name.clone(), serde_json::Value::Object(prop));

            if p.required {
                required_fields.push(serde_json::Value::String(p.name.clone()));
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required_fields,
        })
    }

    pub fn to_mcp_definition(&self, scheme: &str) -> McpToolDefinition {
        let mut desc = self.description.clone();
        if let Some(ref see) = self.see_page {
            desc.push_str(&format!(" (see: {scheme}://doc/{see})"));
        }
        McpToolDefinition {
            name: self.name.clone(),
            description: desc,
            input_schema: self.input_schema(),
        }
    }

    pub fn validate_args(&self, args: &serde_json::Value, scheme: &str) -> Result<(), ToolResult> {
        let obj = match args.as_object() {
            Some(o) => o,
            None => {
                let mut err = "Arguments must be a JSON object.".to_string();
                if let Some(ref see) = self.see_page {
                    err.push_str(&format!(" See {scheme}://doc/{see} for usage."));
                }
                return Err(ToolResult::error(err));
            }
        };

        for p in &self.params {
            let val = obj.get(&p.name);
            if val.is_none() || val == Some(&serde_json::Value::Null) {
                if p.required {
                    let mut err = format!("Missing required argument '{}'.", p.name);
                    if let Some(ref see) = self.see_page {
                        err.push_str(&format!(" See {scheme}://doc/{see} for usage."));
                    }
                    return Err(ToolResult::error(err));
                }
                continue;
            }
            let val = val.unwrap();

            let type_ok = match &p.param_type {
                ParamType::String => val.is_string(),
                ParamType::Integer { min, max } => {
                    if let Some(i) = val.as_i64() {
                        let min_ok = min.is_none_or(|m| i >= m);
                        let max_ok = max.is_none_or(|m| i <= m);
                        min_ok && max_ok
                    } else {
                        false
                    }
                }
                ParamType::Number => val.is_number(),
                ParamType::Boolean => val.is_boolean(),
                ParamType::Enum(variants) => {
                    if let Some(s) = val.as_str() {
                        variants.iter().any(|v| v == s)
                    } else {
                        false
                    }
                }
                ParamType::StringArray => {
                    if let Some(arr) = val.as_array() {
                        arr.iter().all(|item| item.is_string())
                    } else {
                        false
                    }
                }
            };

            if !type_ok {
                let expected = match &p.param_type {
                    ParamType::String => "a string".to_string(),
                    ParamType::Integer { min, max } => match (min, max) {
                        (Some(min_v), Some(max_v)) => {
                            format!("an integer between {min_v} and {max_v}")
                        }
                        (Some(min_v), None) => format!("an integer >= {min_v}"),
                        (None, Some(max_v)) => format!("an integer <= {max_v}"),
                        (None, None) => "an integer".to_string(),
                    },
                    ParamType::Number => "a number".to_string(),
                    ParamType::Boolean => "a boolean".to_string(),
                    ParamType::Enum(variants) => format!("one of {:?}", variants),
                    ParamType::StringArray => "an array of strings".to_string(),
                };
                let mut err = format!(
                    "Invalid argument '{}': expected {expected}, got {val}.",
                    p.name
                );
                if let Some(ref see) = self.see_page {
                    err.push_str(&format!(" See {scheme}://doc/{see} for usage."));
                }
                return Err(ToolResult::error(err));
            }
        }

        Ok(())
    }
}

pub struct ToolBuilder {
    name: String,
    description: String,
    params: Vec<ToolParam>,
    see_page: Option<String>,
    examples: Vec<serde_json::Value>,
    handler: Option<ToolHandler>,
    skip_validation: bool,
}

impl ToolBuilder {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            params: Vec::new(),
            see_page: None,
            examples: Vec::new(),
            handler: None,
            skip_validation: false,
        }
    }

    pub fn param(
        mut self,
        name: impl Into<String>,
        param_type: ParamType,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        self.params.push(ToolParam {
            name: name.into(),
            param_type,
            description: description.into(),
            required,
        });
        self
    }

    pub fn see(mut self, page_id: impl Into<String>) -> Self {
        self.see_page = Some(page_id.into());
        self
    }

    pub fn example(mut self, example: serde_json::Value) -> Self {
        self.examples.push(example);
        self
    }

    pub fn skip_validation(mut self, skip: bool) -> Self {
        self.skip_validation = skip;
        self
    }

    pub fn handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(serde_json::Value) -> HandlerResult<ToolResult> + Send + Sync + 'static,
    {
        self.handler = Some(Arc::new(handler));
        self
    }

    pub fn build(self) -> Tool {
        Tool {
            name: self.name,
            description: self.description,
            params: self.params,
            see_page: self.see_page,
            examples: self.examples,
            skip_validation: self.skip_validation,
            handler: self.handler.unwrap_or_else(|| {
                Arc::new(|_| ToolResult::error("Handler not implemented").into())
            }),
        }
    }
}

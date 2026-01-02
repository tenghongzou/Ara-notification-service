//! Variable substitution engine for templates

use super::types::{TemplateError, TemplateResult};

/// Substitute {{variable}} placeholders in a JSON value
pub fn substitute_variables(
    template: &serde_json::Value,
    variables: &serde_json::Value,
) -> TemplateResult<serde_json::Value> {
    let vars = match variables {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(TemplateError::SubstitutionFailed(
                "Variables must be an object".to_string(),
            ))
        }
    };

    substitute_value(template, vars)
}

fn substitute_value(
    value: &serde_json::Value,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> TemplateResult<serde_json::Value> {
    match value {
        serde_json::Value::String(s) => {
            Ok(serde_json::Value::String(substitute_string(s, variables)))
        }
        serde_json::Value::Array(arr) => {
            let rendered: Result<Vec<_>, _> =
                arr.iter().map(|v| substitute_value(v, variables)).collect();
            Ok(serde_json::Value::Array(rendered?))
        }
        serde_json::Value::Object(obj) => {
            let mut rendered = serde_json::Map::new();
            for (key, val) in obj {
                let rendered_key = substitute_string(key, variables);
                let rendered_val = substitute_value(val, variables)?;
                rendered.insert(rendered_key, rendered_val);
            }
            Ok(serde_json::Value::Object(rendered))
        }
        // Numbers, booleans, null are passed through as-is
        _ => Ok(value.clone()),
    }
}

fn substitute_string(
    template: &str,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut result = template.to_string();

    // Find all {{variable}} patterns and replace them
    for (key, value) in variables {
        let pattern = format!("{{{{{}}}}}", key);
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "".to_string(),
            // For arrays and objects, use JSON representation
            _ => value.to_string(),
        };
        result = result.replace(&pattern, &replacement);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_substitute_simple() {
        let template = json!({
            "message": "Hello, {{name}}!"
        });

        let variables = json!({
            "name": "World"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["message"], "Hello, World!");
    }

    #[test]
    fn test_substitute_multiple() {
        let template = json!({
            "title": "Order {{order_id}} shipped",
            "body": "Your order {{order_id}} is being delivered by {{carrier}}"
        });

        let variables = json!({
            "order_id": "ORD-123",
            "carrier": "FedEx"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["title"], "Order ORD-123 shipped");
        assert_eq!(
            result["body"],
            "Your order ORD-123 is being delivered by FedEx"
        );
    }

    #[test]
    fn test_substitute_nested() {
        let template = json!({
            "notification": {
                "title": "Hello {{name}}",
                "data": {
                    "user_id": "{{user_id}}"
                }
            }
        });

        let variables = json!({
            "name": "Alice",
            "user_id": "user-123"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["notification"]["title"], "Hello Alice");
        assert_eq!(result["notification"]["data"]["user_id"], "user-123");
    }

    #[test]
    fn test_substitute_array() {
        let template = json!({
            "items": ["{{item1}}", "{{item2}}"]
        });

        let variables = json!({
            "item1": "First",
            "item2": "Second"
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["items"][0], "First");
        assert_eq!(result["items"][1], "Second");
    }

    #[test]
    fn test_substitute_number_variable() {
        let template = json!({
            "count": "You have {{count}} items"
        });

        let variables = json!({
            "count": 42
        });

        let result = substitute_variables(&template, &variables).unwrap();
        assert_eq!(result["count"], "You have 42 items");
    }
}

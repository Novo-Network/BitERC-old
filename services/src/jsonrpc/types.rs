use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RPCRequest<T>
where
    T: Clone,
{
    pub jsonrpc: String,
    pub method: String,
    pub params: T,
    pub id: Value,
}

impl<T> RPCRequest<T>
where
    T: Clone,
{
    pub fn new(method: &str, params: T) -> Self {
        Self {
            jsonrpc: String::from("2.0"),
            method: String::from(method),
            params,
            id: Value::from(1),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RPCResponse<T> {
    pub jsonrpc: String,
    pub result: Option<T>,
    pub error: Option<RPCError>,
    pub id: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct RPCResponseResult<T> {
    pub jsonrpc: String,
    pub result: Option<T>,
    pub id: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct RPCResponseError {
    pub jsonrpc: String,
    pub error: RPCError,
    pub id: Value,
}

impl<T> RPCResponse<T> {
    pub fn result(id: Value, t: Option<T>) -> Self {
        Self {
            jsonrpc: String::from("2.0"),
            result: t,
            error: None,
            id,
        }
    }

    pub fn error(id: Value, e: RPCError) -> Self {
        Self {
            jsonrpc: String::from("2.0"),
            result: None,
            error: Some(e),
            id,
        }
    }
}
impl<T> RPCResponse<T>
where
    T: Serialize,
{
    pub fn into_value(self) -> Result<Value> {
        if let Some(e) = self.error {
            let v = RPCResponseError {
                id: self.id,
                jsonrpc: self.jsonrpc,
                error: e,
            };
            Ok(serde_json::to_value(v)?)
        } else {
            let v = RPCResponseResult {
                id: self.id,
                jsonrpc: self.jsonrpc,
                result: self.result,
            };

            Ok(serde_json::to_value(v)?)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RPCError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl RPCError {
    pub fn unknown_method() -> Self {
        RPCError {
            code: -32601,
            message: String::from("Method not found"),
            data: None,
        }
    }

    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: String::from("Parse error"),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: String::from("Invalid params"),
            data: None,
        }
    }

    pub fn internal_error(data: String) -> Self {
        Self {
            code: -32603,
            message: String::from("Internal error"),
            data: Some(data),
        }
    }
}

pub type RPCResult<T> = std::result::Result<T, RPCError>;

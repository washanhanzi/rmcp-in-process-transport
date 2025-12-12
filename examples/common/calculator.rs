#![allow(dead_code)]

use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use tracing::{debug, info};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SumRequest {
    #[schemars(description = "the left hand side number")]
    pub a: i32,
    pub b: i32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SubRequest {
    #[schemars(description = "the left hand side number")]
    pub a: i32,
    #[schemars(description = "the right hand side number")]
    pub b: i32,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct SubResponse {
    pub result: i32,
}

#[derive(Debug, Clone)]
pub struct Calculator {
    pub dummy_data: String,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Calculator {
    pub fn new(dummy_data: String) -> Self {
        debug!(dummy_data = %dummy_data, "Constructing Calculator service");
        Self {
            dummy_data,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Calculate the sum of two numbers")]
    fn sum(&self, Parameters(SumRequest { a, b }): Parameters<SumRequest>) -> String {
        let result = a + b;
        info!(a, b, result, "sum tool invoked");
        result.to_string()
    }

    #[tool(description = "Calculate the difference of two numbers")]
    fn sub(&self, Parameters(SubRequest { a, b }): Parameters<SubRequest>) -> Json<SubResponse> {
        let result = a - b;
        info!(a, b, result, "sub tool invoked");
        Json(SubResponse { result })
    }
}

#[tool_handler]
impl ServerHandler for Calculator {
    fn get_info(&self) -> ServerInfo {
        debug!(dummy_data = %self.dummy_data, "Server get_info called");
        ServerInfo {
            instructions: Some(format!(
                "A simple calculator with dummy data: {}",
                self.dummy_data
            )),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

use std::collections::HashMap;

use rmcp::{ServiceExt, model::CallToolRequestParam};
use rmcp_in_process_transport::in_process::TokioInProcess;

mod common;
use common::calculator::Calculator;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up logging - enable all log levels
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting in-process multiple clients example");

    // Create multiple clients in a HashMap
    let mut client_list = HashMap::new();

    // Create 10 client-server pairs
    for idx in 0..10 {
        tracing::info!("Creating client-server pair {}", idx);

        // Create and start an in-process service, using the TokioInProcess API
        // which is similar to TokioChildProcess
        let calculator = Calculator {
            dummy_data: format!("Client #{}", idx),
        };
        let service = ().into_dyn().serve(TokioInProcess::new(calculator).serve().await?).await?;

        tracing::info!("Client {}: Created successfully", idx);
        client_list.insert(idx, service);
    }

    tracing::info!("Created {} clients", client_list.len());

    // Perform operations on each client
    for (idx, service) in client_list.iter() {
        tracing::info!("Testing client {}", idx);

        // Get server info
        let server_info = service.peer_info();
        tracing::info!("Client {}: Server info: {:?}", idx, server_info);

        // List tools
        match service.list_tools(Default::default()).await {
            Ok(tools) => {
                tracing::info!("Client {}: Available tools: {:?}", idx, tools);
            }
            Err(e) => {
                tracing::error!("Client {}: Failed to get tools: {:?}", idx, e);
                continue;
            }
        }

        // Call the sum tool with arguments
        match service
            .call_tool(CallToolRequestParam {
                name: "sum".into(),
                arguments: serde_json::json!({ "a": idx, "b": 10 })
                    .as_object()
                    .cloned(),
            })
            .await
        {
            Ok(result) => {
                tracing::info!("Client {}: Sum result: {:?}", idx, result);
            }
            Err(e) => {
                tracing::error!("Client {}: Failed to call tool: {:?}", idx, e);
            }
        }
    }

    // Clean up all clients
    tracing::info!("Cleaning up all clients");
    for (idx, service) in client_list {
        match service.cancel().await {
            Ok(_) => tracing::info!("Client {}: Successfully cancelled", idx),
            Err(e) => tracing::error!("Client {}: Error cancelling: {:?}", idx, e),
        }
    }

    tracing::info!("Example completed");
    Ok(())
}

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
        let calculator = Calculator::new(format!("Client #{}", idx));
        let tokio_in_process = TokioInProcess::new(calculator).await?;
        let service = ().into_dyn().serve(tokio_in_process).await?;

        tracing::info!("Client {}: Created successfully", idx);
        client_list.insert(idx, service);
    }

    tracing::info!("✅ Successfully created {} clients", client_list.len());

    // Perform operations on each client
    let mut successful_operations = 0;
    for (idx, service) in client_list.iter() {
        tracing::info!("Testing client {}", idx);

        // Get server info
        let server_info = service.peer_info();
        tracing::info!("Client {}: Server info: {:?}", idx, server_info);

        // List tools
        match service.list_tools(Default::default()).await {
            Ok(tools) => {
                tracing::info!(
                    "Client {}: ✅ Listed {} tools successfully",
                    idx,
                    tools.tools.len()
                );
            }
            Err(e) => {
                tracing::error!("Client {}: ❌ Failed to get tools: {:?}", idx, e);
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
                if let Some(content) = result.content.first() {
                    match &content.raw {
                        rmcp::model::RawContent::Text(text) => {
                            tracing::info!(
                                "Client {}: ✅ Tool call successful: {} + 10 = {}",
                                idx,
                                idx,
                                text.text
                            );
                            successful_operations += 1;
                        }
                        _ => {
                            tracing::info!(
                                "Client {}: ✅ Tool call successful (non-text result)",
                                idx
                            );
                            successful_operations += 1;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Client {}: ❌ Failed to call tool: {:?}", idx, e);
            }
        }

        // Call the sub tool with arguments (structured result)
        match service
            .call_tool(CallToolRequestParam {
                name: "sub".into(),
                arguments: serde_json::json!({ "a": idx, "b": 3 })
                    .as_object()
                    .cloned(),
            })
            .await
        {
            Ok(result) => {
                if let Some(structured) = result.structured_content {
                    tracing::info!(
                        "Client {}: ✅ Tool 'sub' call successful: {} - 3 = {}",
                        idx,
                        idx,
                        structured
                    );
                    successful_operations += 1;
                } else if let Some(content) = result.content.first() {
                    match &content.raw {
                        rmcp::model::RawContent::Text(text) => {
                            tracing::info!(
                                "Client {}: ✅ Tool 'sub' call successful (text): {} - 3 = {}",
                                idx,
                                idx,
                                text.text
                            );
                            successful_operations += 1;
                        }
                        _ => {
                            tracing::info!(
                                "Client {}: ✅ Tool 'sub' call successful (non-text result)",
                                idx
                            );
                            successful_operations += 1;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Client {}: ❌ Failed to call tool 'sub': {:?}", idx, e);
            }
        }
    }

    tracing::info!(
        "✅ Operations completed: {}/{} clients performed successful tool calls",
        successful_operations,
        client_list.len()
    );

    // Clean up all clients
    tracing::info!("🧹 Cleaning up all clients");
    let mut successful_cleanups = 0;
    for (idx, service) in client_list {
        match service.cancel().await {
            Ok(_) => {
                tracing::info!("Client {}: ✅ Successfully cancelled", idx);
                successful_cleanups += 1;
            }
            Err(e) => tracing::error!("Client {}: ❌ Error cancelling: {:?}", idx, e),
        }
    }

    tracing::info!(
        "✅ Cleanup completed: {}/{} clients cancelled successfully",
        successful_cleanups,
        10
    );
    tracing::info!("🎉 IN-PROCESS TRANSPORT EXAMPLE COMPLETED SUCCESSFULLY! 🎉");
    tracing::info!(
        "📊 Summary: {} clients created, {} tool operations, {} cleanups",
        10,
        successful_operations,
        successful_cleanups
    );
    Ok(())
}

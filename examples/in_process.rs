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
        let expected_sum = idx + 10;
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
                            let actual: i32 = text.text.parse().unwrap_or(-1);
                            if actual == expected_sum {
                                tracing::info!(
                                    "[E2E_SUM_PASS] Client {}: sum({} + 10) = {} (expected: {})",
                                    idx,
                                    idx,
                                    actual,
                                    expected_sum
                                );
                                successful_operations += 1;
                            } else {
                                tracing::error!(
                                    "[E2E_SUM_FAIL] Client {}: sum({} + 10) = {} (expected: {})",
                                    idx,
                                    idx,
                                    actual,
                                    expected_sum
                                );
                            }
                        }
                        _ => {
                            tracing::error!(
                                "[E2E_SUM_FAIL] Client {}: unexpected non-text result",
                                idx
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("[E2E_SUM_FAIL] Client {}: call failed: {:?}", idx, e);
            }
        }

        // Call the sub tool with arguments (structured result)
        let expected_sub = idx - 3;
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
                if let Some(structured) = &result.structured_content {
                    // Parse the structured result to verify the value
                    if let Some(result_val) = structured.get("result").and_then(|v| v.as_i64()) {
                        if result_val == expected_sub as i64 {
                            tracing::info!(
                                "[E2E_SUB_PASS] Client {}: sub({} - 3) = {} (expected: {})",
                                idx,
                                idx,
                                result_val,
                                expected_sub
                            );
                            successful_operations += 1;
                        } else {
                            tracing::error!(
                                "[E2E_SUB_FAIL] Client {}: sub({} - 3) = {} (expected: {})",
                                idx,
                                idx,
                                result_val,
                                expected_sub
                            );
                        }
                    } else {
                        tracing::error!(
                            "[E2E_SUB_FAIL] Client {}: could not parse structured result: {}",
                            idx,
                            structured
                        );
                    }
                } else if let Some(content) = result.content.first() {
                    match &content.raw {
                        rmcp::model::RawContent::Text(text) => {
                            tracing::error!(
                                "[E2E_SUB_FAIL] Client {}: expected structured result, got text: {}",
                                idx,
                                text.text
                            );
                        }
                        _ => {
                            tracing::error!(
                                "[E2E_SUB_FAIL] Client {}: expected structured result, got other",
                                idx
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("[E2E_SUB_FAIL] Client {}: call failed: {:?}", idx, e);
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
        "Cleanup completed: {}/{} clients cancelled successfully",
        successful_cleanups,
        10
    );

    // Final E2E verification
    let expected_operations = 20; // 10 clients * 2 tools (sum + sub)
    if successful_operations == expected_operations && successful_cleanups == 10 {
        tracing::info!(
            "[E2E_TEST_PASS] All tests passed: {} tool operations, {} cleanups",
            successful_operations,
            successful_cleanups
        );
    } else {
        tracing::error!(
            "[E2E_TEST_FAIL] Expected {} operations and 10 cleanups, got {} operations and {} cleanups",
            expected_operations,
            successful_operations,
            successful_cleanups
        );
    }
    Ok(())
}

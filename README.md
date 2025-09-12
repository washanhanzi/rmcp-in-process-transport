# rmcp-in-process-transport

use tokio task instead of child process to start the mcp.

```rust
    // Create and start an in-process service, using the TokioInProcess API
    // which is similar to TokioChildProcess
    let calculator = Calculator::new(format!("Client #{}", idx));
    let tokio_in_process = TokioInProcess::new(calculator).await?;
    let service = ().into_dyn().serve(tokio_in_process).await?;
```
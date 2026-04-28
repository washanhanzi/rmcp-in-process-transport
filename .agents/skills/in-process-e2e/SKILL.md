---
name: in-process-e2e
description: Run and verify the rmcp-in-process-transport in-process transport end-to-end example. Use when asked to run the E2E check, validate examples/in_process.rs, verify dependency or Cargo.lock changes against the example, or confirm the [E2E_*] pass/fail markers.
---

# In-process E2E

## Overview

Run the in-process transport example and verify its explicit E2E log markers. Treat a zero exit code as necessary but not sufficient; the pass/fail markers determine the result.

## Command

Run:

```bash
RUST_LOG=info cargo run --example in_process 2>&1
```

When specifically validating a lockfile or dependency change, prefer the locked form:

```bash
RUST_LOG=info cargo run --locked --example in_process 2>&1
```

## Pass Criteria

The run passes only if all of these are true:

- The binary exits successfully.
- There are 10 `[E2E_SUM_PASS]` logs, one for each client `0` through `9`.
- There are 10 `[E2E_SUB_PASS]` logs, one for each client `0` through `9`.
- The final `[E2E_TEST_PASS]` log appears:

```text
[E2E_TEST_PASS] All tests passed: 20 tool operations, 10 cleanups
```

## Failure Criteria

Treat the run as failed if any of these happen:

- Any `[E2E_SUM_FAIL]` or `[E2E_SUB_FAIL]` log appears.
- `[E2E_TEST_FAIL]` appears instead of `[E2E_TEST_PASS]`.
- The binary crashes or panics.
- The expected count of pass markers is incomplete, even if the command exits with status 0.

## Quick Verification

To inspect only the E2E markers:

```bash
RUST_LOG=info cargo run --example in_process 2>&1 | grep -E '\[E2E_'
```

The expected marker summary is 21 lines total: 10 `SUM_PASS`, 10 `SUB_PASS`, and 1 `TEST_PASS`, with no `FAIL` lines.

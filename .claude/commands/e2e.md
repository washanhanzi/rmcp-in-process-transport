Run the end-to-end test for the in-process transport.

Execute the following command:

```bash
RUST_LOG=info cargo run --example in_process 2>&1
```

## Test Pass Criteria

The test is considered **PASSED** only if ALL of the following logs are present:

1. **10 `[E2E_SUM_PASS]` logs** - one for each client (0-9), verifying the sum tool works correctly:
   ```
   [E2E_SUM_PASS] Client X: sum(X + 10) = Y (expected: Y)
   ```

2. **10 `[E2E_SUB_PASS]` logs** - one for each client (0-9), verifying the sub tool works correctly with structured output:
   ```
   [E2E_SUB_PASS] Client X: sub(X - 3) = Y (expected: Y)
   ```

3. **Final `[E2E_TEST_PASS]` log** - confirming all operations completed:
   ```
   [E2E_TEST_PASS] All tests passed: 20 tool operations, 10 cleanups
   ```

## Test Failure

The test is considered **FAILED** if:
- Any `[E2E_SUM_FAIL]` or `[E2E_SUB_FAIL]` logs appear
- The `[E2E_TEST_FAIL]` log appears instead of `[E2E_TEST_PASS]`
- The binary crashes or panics

## Quick Verification

You can grep for pass/fail markers:
```bash
RUST_LOG=info cargo run --example in_process 2>&1 | grep -E '\[E2E_'
```

Expected output should show 10 SUM_PASS, 10 SUB_PASS, and 1 TEST_PASS (21 total lines, no FAIL lines).

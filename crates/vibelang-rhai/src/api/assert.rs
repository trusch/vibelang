//! Assertion functions for testing VibeLang scripts.
//!
//! This module provides assertion functions that can be used in integration tests.
//! Assertions print parseable output that the test runner can analyze.
//!
//! Output format:
//! - `[PASS] message` - Assertion passed
//! - `[FAIL] message` - Assertion failed
//! - `[TEST:name] Starting test` - Test suite started
//! - `[DONE] passed/total` - Test suite finished

use rhai::{Array, Dynamic, Engine, EvalAltResult};
use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, Ordering};

thread_local! {
    /// Test state tracking
    static TEST_STATE: RefCell<TestState> = RefCell::new(TestState::default());
}

/// Global exit code for test termination.
/// Set via `exit(code)` in scripts.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Check if an exit was requested and get the exit code.
pub fn get_exit_code() -> Option<i32> {
    let code = EXIT_CODE.load(Ordering::SeqCst);
    if code == i32::MIN {
        None // No exit requested
    } else {
        Some(code)
    }
}

/// Reset the exit code (call before script execution).
pub fn reset_exit_code() {
    EXIT_CODE.store(i32::MIN, Ordering::SeqCst);
}

#[derive(Default)]
struct TestState {
    test_name: String,
    passed: u32,
    failed: u32,
    fail_fast: bool,
}

/// Register assertion functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Test suite management
    engine.register_fn("test_start", test_start);
    engine.register_fn("test_end", test_end);
    engine.register_fn("test_fail_fast", test_fail_fast);

    // Exit function for integration tests
    engine.register_fn("exit", script_exit);
    engine.register_fn("exit", script_exit_with_code);
    engine.register_fn("test_end_and_exit", test_end_and_exit);

    // Basic assertions
    engine.register_fn("assert", assert_true);
    engine.register_fn("assert_true", assert_true);
    engine.register_fn("assert_false", assert_false);

    // Equality assertions
    engine.register_fn("assert_eq", assert_eq_int);
    engine.register_fn("assert_eq", assert_eq_float);
    engine.register_fn("assert_eq", assert_eq_string);
    engine.register_fn("assert_eq", assert_eq_bool);
    engine.register_fn("assert_eq", assert_eq_dynamic);

    // Inequality assertions
    engine.register_fn("assert_ne", assert_ne_int);
    engine.register_fn("assert_ne", assert_ne_float);
    engine.register_fn("assert_ne", assert_ne_string);

    // Numeric comparisons
    engine.register_fn("assert_gt", assert_gt_int);
    engine.register_fn("assert_gt", assert_gt_float);
    engine.register_fn("assert_lt", assert_lt_int);
    engine.register_fn("assert_lt", assert_lt_float);
    engine.register_fn("assert_gte", assert_gte_int);
    engine.register_fn("assert_gte", assert_gte_float);
    engine.register_fn("assert_lte", assert_lte_int);
    engine.register_fn("assert_lte", assert_lte_float);

    // Approximate equality (for floats)
    engine.register_fn("assert_approx", assert_approx);
    engine.register_fn("assert_approx", assert_approx_with_epsilon);

    // Array assertions
    engine.register_fn("assert_len", assert_len);
    engine.register_fn("assert_contains", assert_contains_int);
    engine.register_fn("assert_contains", assert_contains_string);
    engine.register_fn("assert_empty", assert_empty);
    engine.register_fn("assert_not_empty", assert_not_empty);

    // Range assertions
    engine.register_fn("assert_in_range", assert_in_range_int);
    engine.register_fn("assert_in_range", assert_in_range_float);

    // String assertions
    engine.register_fn("assert_starts_with", assert_starts_with);
    engine.register_fn("assert_ends_with", assert_ends_with);
    engine.register_fn("assert_contains_str", assert_contains_str);
}

// =============================================================================
// Test Suite Management
// =============================================================================

/// Start a test suite with a name.
pub fn test_start(name: &str) {
    TEST_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.test_name = name.to_string();
        s.passed = 0;
        s.failed = 0;
    });
    println!("[TEST:{}] Starting test", name);
}

/// End the current test suite and print summary.
pub fn test_end() -> bool {
    TEST_STATE.with(|state| {
        let s = state.borrow();
        let total = s.passed + s.failed;
        println!("[DONE] {}/{}", s.passed, total);
        s.failed == 0
    })
}

/// Enable fail-fast mode (stop on first failure).
pub fn test_fail_fast(enabled: bool) {
    TEST_STATE.with(|state| {
        state.borrow_mut().fail_fast = enabled;
    });
}

// =============================================================================
// Exit Functions
// =============================================================================

/// Exit the script with exit code 0.
///
/// This signals to the test runner that the script completed successfully.
pub fn script_exit() -> Result<(), Box<EvalAltResult>> {
    script_exit_with_code(0)
}

/// End the test suite and exit with the appropriate exit code.
///
/// This is a convenience function that calls test_end() and then exit().
/// Returns exit code 0 if all tests passed, 1 if any failed.
pub fn test_end_and_exit() -> Result<(), Box<EvalAltResult>> {
    let all_passed = test_end();
    script_exit_with_code(if all_passed { 0 } else { 1 })
}

/// Exit the script with a specific exit code.
///
/// This signals to the test runner to terminate with the given exit code.
/// - exit(0) - Success
/// - exit(1) - Failure (or any non-zero code)
pub fn script_exit_with_code(code: i64) -> Result<(), Box<EvalAltResult>> {
    EXIT_CODE.store(code as i32, Ordering::SeqCst);
    // Return a special error that signals exit
    Err(Box::new(EvalAltResult::Return(
        format!("exit({})", code).into(),
        rhai::Position::NONE,
    )))
}

// =============================================================================
// Internal Helpers
// =============================================================================

fn record_pass(msg: &str) {
    TEST_STATE.with(|state| {
        state.borrow_mut().passed += 1;
    });
    println!("[PASS] {}", msg);
}

fn record_fail(msg: &str) -> Result<(), Box<EvalAltResult>> {
    let should_fail_fast = TEST_STATE.with(|state| {
        let mut s = state.borrow_mut();
        s.failed += 1;
        s.fail_fast
    });
    println!("[FAIL] {}", msg);

    if should_fail_fast {
        Err(format!("Assertion failed: {}", msg).into())
    } else {
        Ok(())
    }
}

// =============================================================================
// Basic Assertions
// =============================================================================

/// Assert that a condition is true.
pub fn assert_true(condition: bool, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if condition {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected true, got false)", msg))
    }
}

/// Assert that a condition is false.
pub fn assert_false(condition: bool, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if !condition {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected false, got true)", msg))
    }
}

// =============================================================================
// Equality Assertions
// =============================================================================

/// Assert two integers are equal.
pub fn assert_eq_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a == b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} == {})", msg, a, b))
    }
}

/// Assert two floats are equal.
pub fn assert_eq_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if (a - b).abs() < f64::EPSILON {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} == {})", msg, a, b))
    }
}

/// Assert two strings are equal.
pub fn assert_eq_string(a: &str, b: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a == b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected '{}' == '{}')", msg, a, b))
    }
}

/// Assert two booleans are equal.
pub fn assert_eq_bool(a: bool, b: bool, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a == b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} == {})", msg, a, b))
    }
}

/// Assert two dynamic values are equal (generic fallback).
pub fn assert_eq_dynamic(a: Dynamic, b: Dynamic, msg: &str) -> Result<(), Box<EvalAltResult>> {
    // Try integer comparison
    if let (Ok(ai), Ok(bi)) = (a.as_int(), b.as_int()) {
        return assert_eq_int(ai, bi, msg);
    }
    // Try float comparison
    if let (Ok(af), Ok(bf)) = (a.as_float(), b.as_float()) {
        return assert_eq_float(af, bf, msg);
    }
    // Try string comparison
    if let (Ok(astr), Ok(bstr)) = (a.clone().into_string(), b.clone().into_string()) {
        return assert_eq_string(&astr, &bstr, msg);
    }
    // Try bool comparison
    if let (Ok(ab), Ok(bb)) = (a.as_bool(), b.as_bool()) {
        return assert_eq_bool(ab, bb, msg);
    }

    // Fallback to debug comparison
    let a_str = format!("{:?}", a);
    let b_str = format!("{:?}", b);
    if a_str == b_str {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {:?} == {:?})", msg, a, b))
    }
}

// =============================================================================
// Inequality Assertions
// =============================================================================

/// Assert two integers are not equal.
pub fn assert_ne_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a != b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} != {})", msg, a, b))
    }
}

/// Assert two floats are not equal.
pub fn assert_ne_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if (a - b).abs() >= f64::EPSILON {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} != {})", msg, a, b))
    }
}

/// Assert two strings are not equal.
pub fn assert_ne_string(a: &str, b: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a != b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected '{}' != '{}')", msg, a, b))
    }
}

// =============================================================================
// Numeric Comparison Assertions
// =============================================================================

/// Assert a > b (integers).
pub fn assert_gt_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a > b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} > {})", msg, a, b))
    }
}

/// Assert a > b (floats).
pub fn assert_gt_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a > b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} > {})", msg, a, b))
    }
}

/// Assert a < b (integers).
pub fn assert_lt_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a < b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} < {})", msg, a, b))
    }
}

/// Assert a < b (floats).
pub fn assert_lt_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a < b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} < {})", msg, a, b))
    }
}

/// Assert a >= b (integers).
pub fn assert_gte_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a >= b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} >= {})", msg, a, b))
    }
}

/// Assert a >= b (floats).
pub fn assert_gte_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a >= b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} >= {})", msg, a, b))
    }
}

/// Assert a <= b (integers).
pub fn assert_lte_int(a: i64, b: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a <= b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} <= {})", msg, a, b))
    }
}

/// Assert a <= b (floats).
pub fn assert_lte_float(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if a <= b {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected {} <= {})", msg, a, b))
    }
}

// =============================================================================
// Approximate Equality
// =============================================================================

/// Assert two floats are approximately equal (default epsilon = 0.0001).
pub fn assert_approx(a: f64, b: f64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    assert_approx_with_epsilon(a, b, 0.0001, msg)
}

/// Assert two floats are approximately equal within epsilon.
pub fn assert_approx_with_epsilon(
    a: f64,
    b: f64,
    epsilon: f64,
    msg: &str,
) -> Result<(), Box<EvalAltResult>> {
    if (a - b).abs() <= epsilon {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected {} ~= {} within epsilon {})",
            msg, a, b, epsilon
        ))
    }
}

// =============================================================================
// Array Assertions
// =============================================================================

/// Assert an array has a specific length.
pub fn assert_len(arr: Array, expected: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    let actual = arr.len() as i64;
    if actual == expected {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected len {} but got {})", msg, expected, actual))
    }
}

/// Assert an array contains a specific integer.
pub fn assert_contains_int(arr: Array, value: i64, msg: &str) -> Result<(), Box<EvalAltResult>> {
    let found = arr.iter().any(|v| v.as_int().map(|i| i == value).unwrap_or(false));
    if found {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (array does not contain {})", msg, value))
    }
}

/// Assert an array contains a specific string.
pub fn assert_contains_string(arr: Array, value: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    let found = arr.iter().any(|v| {
        v.clone()
            .into_string()
            .map(|s| s == value)
            .unwrap_or(false)
    });
    if found {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (array does not contain '{}')", msg, value))
    }
}

/// Assert an array is empty.
pub fn assert_empty(arr: Array, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if arr.is_empty() {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected empty array, got {} elements)", msg, arr.len()))
    }
}

/// Assert an array is not empty.
pub fn assert_not_empty(arr: Array, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if !arr.is_empty() {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!("{} (expected non-empty array)", msg))
    }
}

// =============================================================================
// Range Assertions
// =============================================================================

/// Assert a value is within a range (inclusive).
pub fn assert_in_range_int(
    value: i64,
    min: i64,
    max: i64,
    msg: &str,
) -> Result<(), Box<EvalAltResult>> {
    if value >= min && value <= max {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected {} in range [{}, {}])",
            msg, value, min, max
        ))
    }
}

/// Assert a value is within a range (inclusive).
pub fn assert_in_range_float(
    value: f64,
    min: f64,
    max: f64,
    msg: &str,
) -> Result<(), Box<EvalAltResult>> {
    if value >= min && value <= max {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected {} in range [{}, {}])",
            msg, value, min, max
        ))
    }
}

// =============================================================================
// String Assertions
// =============================================================================

/// Assert a string starts with a prefix.
pub fn assert_starts_with(s: &str, prefix: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if s.starts_with(prefix) {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected '{}' to start with '{}')",
            msg, s, prefix
        ))
    }
}

/// Assert a string ends with a suffix.
pub fn assert_ends_with(s: &str, suffix: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if s.ends_with(suffix) {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected '{}' to end with '{}')",
            msg, s, suffix
        ))
    }
}

/// Assert a string contains a substring.
pub fn assert_contains_str(s: &str, substring: &str, msg: &str) -> Result<(), Box<EvalAltResult>> {
    if s.contains(substring) {
        record_pass(msg);
        Ok(())
    } else {
        record_fail(&format!(
            "{} (expected '{}' to contain '{}')",
            msg, s, substring
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reset state before each test
    fn reset_test_state() {
        TEST_STATE.with(|state| {
            let mut s = state.borrow_mut();
            s.test_name = String::new();
            s.passed = 0;
            s.failed = 0;
            s.fail_fast = false;
        });
        reset_exit_code();
    }

    // =========================================================================
    // Exit Function Tests
    // =========================================================================

    #[test]
    fn test_exit_code_initially_none() {
        reset_exit_code();
        assert_eq!(get_exit_code(), None);
    }

    #[test]
    fn test_exit_sets_code() {
        reset_exit_code();
        let _ = script_exit_with_code(42);
        let code = get_exit_code();
        // Code should be set (either 42 or from another test running in parallel)
        assert!(code.is_some(), "exit code should be set");
    }

    #[test]
    fn test_exit_zero() {
        reset_exit_code();
        let result = script_exit();
        assert!(result.is_err()); // exit returns an error to stop execution
        assert_eq!(get_exit_code(), Some(0));
    }

    #[test]
    fn test_exit_with_negative_code() {
        reset_exit_code();
        let _ = script_exit_with_code(-1);
        assert_eq!(get_exit_code(), Some(-1));
    }

    // =========================================================================
    // Test Suite Management Tests
    // =========================================================================

    #[test]
    fn test_test_start_sets_name() {
        reset_test_state();
        test_start("my_test");
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().test_name, "my_test");
        });
    }

    #[test]
    fn test_test_end_returns_true_when_all_pass() {
        reset_test_state();
        test_start("passing_test");
        TEST_STATE.with(|state| {
            let mut s = state.borrow_mut();
            s.passed = 5;
            s.failed = 0;
        });
        assert!(test_end());
    }

    #[test]
    fn test_test_end_returns_false_when_any_fail() {
        reset_test_state();
        test_start("failing_test");
        TEST_STATE.with(|state| {
            let mut s = state.borrow_mut();
            s.passed = 4;
            s.failed = 1;
        });
        assert!(!test_end());
    }

    #[test]
    fn test_test_end_and_exit_success() {
        reset_test_state();
        test_start("success_test");
        TEST_STATE.with(|state| {
            let mut s = state.borrow_mut();
            s.passed = 3;
            s.failed = 0;
        });
        let _ = test_end_and_exit();
        assert_eq!(get_exit_code(), Some(0));
    }

    #[test]
    fn test_test_end_and_exit_failure() {
        reset_test_state();
        test_start("fail_test");
        TEST_STATE.with(|state| {
            let mut s = state.borrow_mut();
            s.passed = 2;
            s.failed = 1;
        });
        let _ = test_end_and_exit();
        assert_eq!(get_exit_code(), Some(1));
    }

    // =========================================================================
    // Assertion Tests
    // =========================================================================

    #[test]
    fn test_assert_true_passes() {
        reset_test_state();
        assert!(assert_true(true, "should pass").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
            assert_eq!(state.borrow().failed, 0);
        });
    }

    #[test]
    fn test_assert_true_fails() {
        reset_test_state();
        // Without fail_fast, it returns Ok but records failure
        assert!(assert_true(false, "should fail").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 0);
            assert_eq!(state.borrow().failed, 1);
        });
    }

    #[test]
    fn test_assert_false_passes() {
        reset_test_state();
        assert!(assert_false(false, "should pass").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_eq_int_passes() {
        reset_test_state();
        assert!(assert_eq_int(42, 42, "equal").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_eq_int_fails() {
        reset_test_state();
        assert!(assert_eq_int(42, 43, "not equal").is_ok()); // Ok but recorded as fail
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().failed, 1);
        });
    }

    #[test]
    fn test_assert_approx_passes() {
        reset_test_state();
        assert!(assert_approx(1.0, 1.00005, "close enough").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_approx_fails() {
        reset_test_state();
        assert!(assert_approx(1.0, 2.0, "not close").is_ok()); // Ok but fail recorded
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().failed, 1);
        });
    }

    #[test]
    fn test_assert_in_range_passes() {
        reset_test_state();
        assert!(assert_in_range_int(5, 0, 10, "in range").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_in_range_fails() {
        reset_test_state();
        assert!(assert_in_range_int(15, 0, 10, "out of range").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().failed, 1);
        });
    }

    #[test]
    fn test_fail_fast_mode() {
        reset_test_state();
        test_fail_fast(true);

        // First failure should return error in fail-fast mode
        let result = assert_true(false, "should fail fast");
        assert!(result.is_err());
    }

    // =========================================================================
    // Array Assertion Tests
    // =========================================================================

    #[test]
    fn test_assert_len_passes() {
        reset_test_state();
        let arr: rhai::Array = vec![1.into(), 2.into(), 3.into()];
        assert!(assert_len(arr, 3, "length check").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_empty_passes() {
        reset_test_state();
        let arr: rhai::Array = vec![];
        assert!(assert_empty(arr, "empty check").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_not_empty_passes() {
        reset_test_state();
        let arr: rhai::Array = vec![1.into()];
        assert!(assert_not_empty(arr, "not empty").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    // =========================================================================
    // String Assertion Tests
    // =========================================================================

    #[test]
    fn test_assert_starts_with_passes() {
        reset_test_state();
        assert!(assert_starts_with("hello world", "hello", "prefix").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_ends_with_passes() {
        reset_test_state();
        assert!(assert_ends_with("hello world", "world", "suffix").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }

    #[test]
    fn test_assert_contains_str_passes() {
        reset_test_state();
        assert!(assert_contains_str("hello world", "lo wo", "contains").is_ok());
        TEST_STATE.with(|state| {
            assert_eq!(state.borrow().passed, 1);
        });
    }
}

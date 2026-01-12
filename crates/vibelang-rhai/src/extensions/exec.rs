//! Shell command execution extension for Rhai.
//!
//! Provides the ability to execute shell commands from VibeLang scripts.
//!
//! # Available Functions
//!
//! - `exec(command)` - Execute command and return output as string
//! - `exec_status(command)` - Execute command and return exit status code
//! - `exec_lines(command)` - Execute command and return output as array of lines
//! - `exec_with_args(program, args)` - Execute program with array of arguments
//! - `shell(script)` - Execute script through the shell
//! - `env_var(name)` - Get environment variable value
//! - `set_env_var(name, value)` - Set environment variable
//! - `cwd()` - Get current working directory
//! - `set_cwd(path)` - Change current working directory
//!
//! # Security Warning
//!
//! This extension allows arbitrary command execution. Only enable it
//! in trusted environments where script authors are trusted.
//!
//! # Example
//!
//! ```rhai
//! // Execute a command and get output
//! let files = exec("ls -la");
//! print(files);
//!
//! // Get exit status
//! let status = exec_status("make build");
//! if status != 0 {
//!     print("Build failed!");
//! }
//!
//! // Execute with arguments (safer)
//! let result = exec_with_args("echo", ["hello", "world"]);
//! ```

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};
use std::process::{Command, Stdio};

/// Register exec functions with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Command execution
    engine.register_fn("exec", exec);
    engine.register_fn("exec_status", exec_status);
    engine.register_fn("exec_lines", exec_lines);
    engine.register_fn("exec_with_args", exec_with_args);
    engine.register_fn("exec_full", exec_full);

    // Shell execution
    engine.register_fn("shell", shell);

    // Environment
    engine.register_fn("env_var", env_var);
    engine.register_fn("env_var_or", env_var_or);
    engine.register_fn("set_env_var", set_env_var);
    engine.register_fn("env_vars", env_vars);

    // Working directory
    engine.register_fn("cwd", cwd);
    engine.register_fn("set_cwd", set_cwd);

    // Process info
    engine.register_fn("pid", pid);
}

// ============================================================================
// Command Execution
// ============================================================================

/// Execute a command and return its stdout as a string.
///
/// The command is parsed as a shell command (split by whitespace).
/// For commands with complex arguments, use `exec_with_args` or `shell`.
pub fn exec(command: &str) -> Result<String, Box<EvalAltResult>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(String::new());
    }

    let output = Command::new(parts[0])
        .args(&parts[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to execute '{}': {}", command, e).into(),
                rhai::Position::NONE,
            ))
        })?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Execute a command and return its exit status code.
pub fn exec_status(command: &str) -> Result<i64, Box<EvalAltResult>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(0);
    }

    let status = Command::new(parts[0])
        .args(&parts[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to execute '{}': {}", command, e).into(),
                rhai::Position::NONE,
            ))
        })?;

    Ok(status.code().unwrap_or(-1) as i64)
}

/// Execute a command and return output as array of lines.
pub fn exec_lines(command: &str) -> Result<Array, Box<EvalAltResult>> {
    let output = exec(command)?;
    Ok(output
        .lines()
        .map(|line| Dynamic::from(line.to_string()))
        .collect())
}

/// Execute a program with explicit arguments (safer than string parsing).
pub fn exec_with_args(program: &str, args: Array) -> Result<String, Box<EvalAltResult>> {
    let args: Vec<String> = args.into_iter().map(|a| a.to_string()).collect();

    let output = Command::new(program)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to execute '{}': {}", program, e).into(),
                rhai::Position::NONE,
            ))
        })?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Execute a command and return full result including stdout, stderr, and status.
///
/// Returns a map with keys: "stdout", "stderr", "status", "success"
pub fn exec_full(command: &str) -> Result<Map, Box<EvalAltResult>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        let mut result = Map::new();
        result.insert("stdout".into(), Dynamic::from(""));
        result.insert("stderr".into(), Dynamic::from(""));
        result.insert("status".into(), Dynamic::from(0_i64));
        result.insert("success".into(), Dynamic::from(true));
        return Ok(result);
    }

    let output = Command::new(parts[0])
        .args(&parts[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            Box::new(EvalAltResult::ErrorRuntime(
                format!("Failed to execute '{}': {}", command, e).into(),
                rhai::Position::NONE,
            ))
        })?;

    let mut result = Map::new();
    result.insert(
        "stdout".into(),
        Dynamic::from(String::from_utf8_lossy(&output.stdout).to_string()),
    );
    result.insert(
        "stderr".into(),
        Dynamic::from(String::from_utf8_lossy(&output.stderr).to_string()),
    );
    result.insert(
        "status".into(),
        Dynamic::from(output.status.code().unwrap_or(-1) as i64),
    );
    result.insert("success".into(), Dynamic::from(output.status.success()));

    Ok(result)
}

// ============================================================================
// Shell Execution
// ============================================================================

/// Execute a script through the system shell.
///
/// On Unix, uses /bin/sh -c. On Windows, uses cmd /C.
/// This allows for shell features like pipes, redirects, etc.
pub fn shell(script: &str) -> Result<String, Box<EvalAltResult>> {
    #[cfg(unix)]
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    #[cfg(windows)]
    let output = Command::new("cmd")
        .arg("/C")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = output.map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to execute shell script: {}", e).into(),
            rhai::Position::NONE,
        ))
    })?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ============================================================================
// Environment Variables
// ============================================================================

/// Get an environment variable value.
pub fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// Get an environment variable value with a default.
pub fn env_var_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Set an environment variable.
pub fn set_env_var(name: &str, value: &str) {
    std::env::set_var(name, value);
}

/// Get all environment variables as a map.
pub fn env_vars() -> Map {
    let mut result = Map::new();
    for (key, value) in std::env::vars() {
        result.insert(key.into(), Dynamic::from(value));
    }
    result
}

// ============================================================================
// Working Directory
// ============================================================================

/// Get current working directory.
pub fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Change current working directory.
pub fn set_cwd(path: &str) -> Result<(), Box<EvalAltResult>> {
    std::env::set_current_dir(path).map_err(|e| {
        Box::new(EvalAltResult::ErrorRuntime(
            format!("Failed to change directory to '{}': {}", path, e).into(),
            rhai::Position::NONE,
        ))
    })
}

// ============================================================================
// Process Info
// ============================================================================

/// Get current process ID.
pub fn pid() -> i64 {
    std::process::id() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_echo() {
        let output = exec("echo hello").unwrap();
        assert!(output.trim() == "hello");
    }

    #[test]
    fn test_exec_status() {
        let status = exec_status("true").unwrap();
        assert_eq!(status, 0);

        let status = exec_status("false").unwrap();
        assert_ne!(status, 0);
    }

    #[test]
    fn test_env_var() {
        set_env_var("TEST_VAR_XYZ", "test_value");
        assert_eq!(env_var("TEST_VAR_XYZ"), "test_value");
    }

    #[test]
    fn test_env_var_or() {
        assert_eq!(env_var_or("NONEXISTENT_VAR_ABC123", "default"), "default");
    }

    #[test]
    fn test_cwd() {
        let dir = cwd();
        assert!(!dir.is_empty());
    }

    #[test]
    fn test_pid() {
        let p = pid();
        assert!(p > 0);
    }
}

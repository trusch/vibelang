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

fn boundary_error(code: &str, token: &str, expected: &str) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "{code} span=0..{} expected={expected} token={token:?}",
            token.len()
        )
        .into(),
        rhai::Position::NONE,
    ))
}

fn require_non_empty<'a>(
    code: &str,
    value: &'a str,
    expected: &str,
) -> Result<&'a str, Box<EvalAltResult>> {
    if value.is_empty() || value.chars().all(char::is_whitespace) {
        Err(boundary_error(code, value, expected))
    } else {
        Ok(value)
    }
}

/// Install the complete exec inventory with strict vibe-api 2 boundaries.
pub(crate) fn register_v2(engine: &mut Engine) {
    register(engine);
    engine
        .register_fn("exec", |command: &str| {
            exec_strict(require_non_empty(
                "extension.exec.empty_command",
                command,
                "non_empty_command",
            )?)
        })
        .register_fn("exec_status", |command: &str| {
            exec_status_strict(require_non_empty(
                "extension.exec.empty_command",
                command,
                "non_empty_command",
            )?)
        })
        .register_fn("exec_lines", |command: &str| {
            exec_lines_strict(require_non_empty(
                "extension.exec.empty_command",
                command,
                "non_empty_command",
            )?)
        })
        .register_fn("exec_full", |command: &str| {
            exec_full_strict(require_non_empty(
                "extension.exec.empty_command",
                command,
                "non_empty_command",
            )?)
        })
        .register_fn("exec_with_args", |program: &str, args: Array| {
            let program =
                require_non_empty("extension.exec.empty_program", program, "non_empty_program")?;
            for (index, arg) in args.iter().enumerate() {
                if !arg.is_string() {
                    return Err(boundary_error(
                        "extension.exec.argument_type",
                        &arg.to_string(),
                        &format!("string_at_index_{index}"),
                    ));
                }
            }
            exec_with_args_strict(program, args)
        })
        .register_fn("shell", |script: &str| {
            shell_strict(require_non_empty(
                "extension.exec.empty_script",
                script,
                "non_empty_script",
            )?)
        })
        .register_fn("env_var", env_var_strict)
        .register_fn("env_var_or", env_var_or_strict)
        .register_fn("set_env_var", set_env_var_strict)
        .register_fn("env_vars", env_vars_strict)
        .register_fn("cwd", cwd_strict)
        .register_fn("set_cwd", set_cwd_strict);
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
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=exec argument=command input={command:?} recovery=legacy_empty_output effective_value=\"\" replacement=use_non_empty_command"
        );
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

fn exec_strict(command: &str) -> Result<String, Box<EvalAltResult>> {
    let mut parts = command.split_whitespace();
    let program = parts.next().ok_or_else(|| {
        boundary_error("extension.exec.empty_command", command, "non_empty_command")
    })?;
    let output = Command::new(program)
        .args(parts)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            boundary_error(
                "extension.exec.spawn_failed",
                command,
                &format!("executable_command_error_{error}"),
            )
        })?;
    String::from_utf8(output.stdout).map_err(|error| {
        boundary_error(
            "extension.exec.stdout_encoding",
            command,
            &format!("utf8_stdout_error_{error}"),
        )
    })
}

/// Execute a command and return its exit status code.
pub fn exec_status(command: &str) -> Result<i64, Box<EvalAltResult>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=exec_status argument=command input={command:?} recovery=legacy_success_status effective_value=0 replacement=use_non_empty_command"
        );
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

    Ok(match status.code() {
        Some(status) => i64::from(status),
        None => {
            log::warn!(
                "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=exec_status argument=command input={command:?} recovery=legacy_signal_status effective_value=-1 replacement=handle_terminated_process"
            );
            -1
        }
    })
}

fn exec_status_strict(command: &str) -> Result<i64, Box<EvalAltResult>> {
    let mut parts = command.split_whitespace();
    let program = parts.next().ok_or_else(|| {
        boundary_error("extension.exec.empty_command", command, "non_empty_command")
    })?;
    let status = Command::new(program)
        .args(parts)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            boundary_error(
                "extension.exec.spawn_failed",
                command,
                &format!("executable_command_error_{error}"),
            )
        })?;
    status.code().map(i64::from).ok_or_else(|| {
        boundary_error(
            "extension.exec.terminated_by_signal",
            command,
            "normal_exit_status",
        )
    })
}

/// Execute a command and return output as array of lines.
pub fn exec_lines(command: &str) -> Result<Array, Box<EvalAltResult>> {
    let output = exec(command)?;
    Ok(output
        .lines()
        .map(|line| Dynamic::from(line.to_string()))
        .collect())
}

fn exec_lines_strict(command: &str) -> Result<Array, Box<EvalAltResult>> {
    Ok(exec_strict(command)?
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

fn exec_with_args_strict(program: &str, args: Array) -> Result<String, Box<EvalAltResult>> {
    let args = args
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            value.into_string().map_err(|value| {
                boundary_error(
                    "extension.exec.argument_type",
                    &value.to_string(),
                    &format!("string_at_index_{index}"),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            boundary_error(
                "extension.exec.spawn_failed",
                program,
                &format!("executable_program_error_{error}"),
            )
        })?;
    String::from_utf8(output.stdout).map_err(|error| {
        boundary_error(
            "extension.exec.stdout_encoding",
            program,
            &format!("utf8_stdout_error_{error}"),
        )
    })
}

/// Execute a command and return full result including stdout, stderr, and status.
///
/// Returns a map with keys: "stdout", "stderr", "status", "success"
pub fn exec_full(command: &str) -> Result<Map, Box<EvalAltResult>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=exec_full argument=command input={command:?} recovery=legacy_empty_success effective_value=status_0 replacement=use_non_empty_command"
        );
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
    let status = match output.status.code() {
        Some(status) => i64::from(status),
        None => {
            log::warn!(
                "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=exec_full argument=command input={command:?} recovery=legacy_signal_status effective_value=-1 replacement=handle_terminated_process"
            );
            -1
        }
    };
    result.insert("status".into(), Dynamic::from(status));
    result.insert("success".into(), Dynamic::from(output.status.success()));

    Ok(result)
}

fn exec_full_strict(command: &str) -> Result<Map, Box<EvalAltResult>> {
    let mut parts = command.split_whitespace();
    let program = parts.next().ok_or_else(|| {
        boundary_error("extension.exec.empty_command", command, "non_empty_command")
    })?;
    let output = Command::new(program)
        .args(parts)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            boundary_error(
                "extension.exec.spawn_failed",
                command,
                &format!("executable_command_error_{error}"),
            )
        })?;
    let status = output.status.code().ok_or_else(|| {
        boundary_error(
            "extension.exec.terminated_by_signal",
            command,
            "normal_exit_status",
        )
    })?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        boundary_error(
            "extension.exec.stdout_encoding",
            command,
            &format!("utf8_stdout_error_{error}"),
        )
    })?;
    let stderr = String::from_utf8(output.stderr).map_err(|error| {
        boundary_error(
            "extension.exec.stderr_encoding",
            command,
            &format!("utf8_stderr_error_{error}"),
        )
    })?;
    let mut result = Map::new();
    result.insert("stdout".into(), Dynamic::from(stdout));
    result.insert("stderr".into(), Dynamic::from(stderr));
    result.insert("status".into(), Dynamic::from(i64::from(status)));
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

fn shell_strict(script: &str) -> Result<String, Box<EvalAltResult>> {
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

    let output = output.map_err(|error| {
        boundary_error(
            "extension.exec.shell_failed",
            script,
            &format!("executable_shell_error_{error}"),
        )
    })?;
    String::from_utf8(output.stdout).map_err(|error| {
        boundary_error(
            "extension.exec.stdout_encoding",
            script,
            &format!("utf8_stdout_error_{error}"),
        )
    })
}

// ============================================================================
// Environment Variables
// ============================================================================

/// Get an environment variable value.
pub fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        log::warn!(
            "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=env_var argument=name input={name:?} recovery=legacy_empty_string effective_value=\"\" replacement=use_env_var_or_or_existing_variable"
        );
        String::new()
    })
}

fn env_var_strict(name: &str) -> Result<String, Box<EvalAltResult>> {
    let name = environment_name_strict(name)?;
    std::env::var(name).map_err(|error| {
        boundary_error(
            "extension.exec.environment_missing",
            name,
            &format!("existing_unicode_environment_value_error_{error}"),
        )
    })
}

/// Get an environment variable value with a default.
pub fn env_var_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn environment_name_strict(name: &str) -> Result<&str, Box<EvalAltResult>> {
    let name = require_non_empty(
        "extension.exec.empty_environment_name",
        name,
        "non_empty_environment_name",
    )?;
    if name.contains(['=', '\0']) {
        return Err(boundary_error(
            "extension.exec.environment_name",
            name,
            "environment_name_without_equals_or_nul",
        ));
    }
    Ok(name)
}

fn env_var_or_strict(name: &str, default: &str) -> Result<String, Box<EvalAltResult>> {
    let name = environment_name_strict(name)?;
    match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(error @ std::env::VarError::NotUnicode(_)) => Err(boundary_error(
            "extension.exec.environment_encoding",
            name,
            &format!("unicode_environment_value_error_{error}"),
        )),
    }
}

/// Set an environment variable.
pub fn set_env_var(name: &str, value: &str) {
    std::env::set_var(name, value);
}

fn set_env_var_strict(name: &str, value: &str) -> Result<(), Box<EvalAltResult>> {
    let name = environment_name_strict(name)?;
    if value.contains('\0') {
        return Err(boundary_error(
            "extension.exec.environment_value",
            value,
            "environment_value_without_nul",
        ));
    }
    std::env::set_var(name, value);
    Ok(())
}

/// Get all environment variables as a map.
pub fn env_vars() -> Map {
    let mut result = Map::new();
    for (key, value) in std::env::vars() {
        result.insert(key.into(), Dynamic::from(value));
    }
    result
}

fn env_vars_strict() -> Result<Map, Box<EvalAltResult>> {
    std::env::vars_os()
        .map(|(key, value)| {
            let key = key.into_string().map_err(|key| {
                boundary_error(
                    "extension.exec.environment_name_encoding",
                    &key.to_string_lossy(),
                    "unicode_environment_name",
                )
            })?;
            let value = value.into_string().map_err(|value| {
                boundary_error(
                    "extension.exec.environment_value_encoding",
                    &value.to_string_lossy(),
                    "unicode_environment_value",
                )
            })?;
            Ok((key.into(), Dynamic::from(value)))
        })
        .collect()
}

// ============================================================================
// Working Directory
// ============================================================================

/// Get current working directory.
pub fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|error| {
            log::warn!(
                "diagnostic.compat.fallback_applied profile=compat.vibelang.v1 function=cwd input=none recovery=legacy_empty_string effective_value=\"\" replacement=handle_current_directory_error_{error}"
            );
            String::new()
        })
}

fn cwd_strict() -> Result<String, Box<EvalAltResult>> {
    let path = std::env::current_dir().map_err(|error| {
        boundary_error(
            "extension.exec.cwd_unavailable",
            "",
            &format!("current_directory_error_{error}"),
        )
    })?;
    path.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        boundary_error(
            "extension.exec.cwd_encoding",
            &path.to_string_lossy(),
            "unicode_current_directory",
        )
    })
}

fn set_cwd_strict(path: &str) -> Result<(), Box<EvalAltResult>> {
    let path = require_non_empty("extension.exec.empty_cwd", path, "non_empty_directory_path")?;
    std::env::set_current_dir(path).map_err(|error| {
        boundary_error(
            "extension.exec.set_cwd",
            path,
            &format!("existing_directory_error_{error}"),
        )
    })
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

    #[test]
    fn v2_rejects_legacy_exec_sentinels() {
        let mut engine = Engine::new();
        register_v2(&mut engine);
        for (script, diagnostic) in [
            (r#"exec("")"#, "extension.exec.empty_command"),
            (
                r#"exec("VIBELANG_EXECUTABLE_MUST_NOT_EXIST_7A31A6D9")"#,
                "extension.exec.spawn_failed",
            ),
            (
                r#"env_var("NONEXISTENT_VAR_VIBELANG_V2")"#,
                "extension.exec.environment_missing",
            ),
            (
                r#"env_var_or("", "fallback")"#,
                "extension.exec.empty_environment_name",
            ),
            (
                r#"set_env_var("INVALID=NAME", "value")"#,
                "extension.exec.environment_name",
            ),
            (r#"set_cwd("")"#, "extension.exec.empty_cwd"),
        ] {
            let error = engine.eval::<Dynamic>(script).unwrap_err().to_string();
            assert!(error.contains(diagnostic), "{script}: {error}");
        }
    }
}

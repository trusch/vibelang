//! Audio backend implementations (PipeWire, JACK).

use super::{AudioError, Port, PortDirection, Result};
use std::process::Command;

/// Capabilities of an audio backend.
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    /// Can list ports
    pub can_list: bool,
    /// Can connect ports
    pub can_connect: bool,
    /// Can disconnect ports
    pub can_disconnect: bool,
    /// Can list connections
    pub can_list_connections: bool,
}

/// Audio backend trait for different audio systems.
pub trait AudioBackend: Send + Sync {
    /// Get the backend name.
    fn name(&self) -> &str;

    /// Get backend capabilities.
    fn capabilities(&self) -> BackendCapabilities;

    /// List all output ports (audio sources).
    fn list_output_ports(&self) -> Result<Vec<Port>>;

    /// List all input ports (audio sinks).
    fn list_input_ports(&self) -> Result<Vec<Port>>;

    /// Connect two ports.
    fn connect(&self, source: &str, destination: &str) -> Result<()>;

    /// Disconnect two ports.
    fn disconnect(&self, source: &str, destination: &str) -> Result<()>;

    /// List all connections for a given port.
    fn list_connections(&self, port: &str) -> Result<Vec<String>>;
}

/// PipeWire backend using pw-link.
pub struct PipeWireBackend;

impl PipeWireBackend {
    /// Check if pw-link is available.
    pub fn is_available() -> bool {
        Command::new("pw-link")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create a new PipeWire backend if available.
    pub fn new() -> Option<Self> {
        if Self::is_available() {
            Some(Self)
        } else {
            None
        }
    }

    fn parse_port_list(&self, output: &str, direction: PortDirection) -> Vec<Port> {
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| Port::parse(line.trim(), direction))
            .collect()
    }
}

impl AudioBackend for PipeWireBackend {
    fn name(&self) -> &str {
        "PipeWire (pw-link)"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            can_list: true,
            can_connect: true,
            can_disconnect: true,
            can_list_connections: true,
        }
    }

    fn list_output_ports(&self) -> Result<Vec<Port>> {
        let output = Command::new("pw-link")
            .arg("-o")
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: "pw-link -o".into(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(AudioError::CommandFailed {
                command: "pw-link -o".into(),
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(self.parse_port_list(&stdout, PortDirection::Output))
    }

    fn list_input_ports(&self) -> Result<Vec<Port>> {
        let output = Command::new("pw-link")
            .arg("-i")
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: "pw-link -i".into(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(AudioError::CommandFailed {
                command: "pw-link -i".into(),
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(self.parse_port_list(&stdout, PortDirection::Input))
    }

    fn connect(&self, source: &str, destination: &str) -> Result<()> {
        let output = Command::new("pw-link")
            .args([source, destination])
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: format!("pw-link {} {}", source, destination),
                reason: e.to_string(),
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already linked" is not an error
        if stderr.contains("already") {
            return Ok(());
        }

        Err(AudioError::ConnectionFailed {
            source: source.into(),
            destination: destination.into(),
            reason: stderr.into_owned(),
        })
    }

    fn disconnect(&self, source: &str, destination: &str) -> Result<()> {
        let output = Command::new("pw-link")
            .args(["-d", source, destination])
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: format!("pw-link -d {} {}", source, destination),
                reason: e.to_string(),
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AudioError::DisconnectionFailed {
            source: source.into(),
            destination: destination.into(),
            reason: stderr.into_owned(),
        })
    }

    fn list_connections(&self, port: &str) -> Result<Vec<String>> {
        let output = Command::new("pw-link")
            .args(["-l", port])
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: format!("pw-link -l {}", port),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Ok(Vec::new()); // Port might not exist, return empty
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter(|line| !line.is_empty() && line.contains(':'))
            .map(|line| line.trim().to_string())
            .collect())
    }
}

/// JACK backend using jack_connect/jack_disconnect/jack_lsp.
pub struct JackBackend;

impl JackBackend {
    /// Check if JACK tools are available.
    pub fn is_available() -> bool {
        Command::new("jack_lsp")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create a new JACK backend if available.
    pub fn new() -> Option<Self> {
        if Self::is_available() {
            Some(Self)
        } else {
            None
        }
    }

    fn parse_jack_lsp_output(&self, output: &str) -> (Vec<Port>, Vec<Port>) {
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut current_port: Option<String> = None;
        let mut current_connections = Vec::new();
        let mut is_output = false;

        for line in output.lines() {
            if line.starts_with(' ') || line.starts_with('\t') {
                // This is a connection or property line
                let trimmed = line.trim();
                if trimmed.contains(':') && !trimmed.starts_with("properties:") {
                    current_connections.push(trimmed.to_string());
                }
            } else if !line.is_empty() {
                // Save previous port
                if let Some(port_name) = current_port.take() {
                    let mut port = Port::parse(&port_name, if is_output {
                        PortDirection::Output
                    } else {
                        PortDirection::Input
                    });
                    port.connections = std::mem::take(&mut current_connections);

                    if is_output {
                        outputs.push(port);
                    } else {
                        inputs.push(port);
                    }
                }

                // New port
                current_port = Some(line.trim().to_string());
                // Determine direction based on port name conventions
                is_output = line.contains(":out")
                    || line.contains(":capture")
                    || line.contains(":monitor")
                    || line.contains(":send");
            }
        }

        // Don't forget the last port
        if let Some(port_name) = current_port {
            let mut port = Port::parse(&port_name, if is_output {
                PortDirection::Output
            } else {
                PortDirection::Input
            });
            port.connections = current_connections;

            if is_output {
                outputs.push(port);
            } else {
                inputs.push(port);
            }
        }

        (outputs, inputs)
    }
}

impl AudioBackend for JackBackend {
    fn name(&self) -> &str {
        "JACK (jack_lsp/jack_connect)"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            can_list: true,
            can_connect: true,
            can_disconnect: true,
            can_list_connections: true,
        }
    }

    fn list_output_ports(&self) -> Result<Vec<Port>> {
        let output = Command::new("jack_lsp")
            .arg("-c")
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: "jack_lsp -c".into(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(AudioError::CommandFailed {
                command: "jack_lsp -c".into(),
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (outputs, _) = self.parse_jack_lsp_output(&stdout);
        Ok(outputs)
    }

    fn list_input_ports(&self) -> Result<Vec<Port>> {
        let output = Command::new("jack_lsp")
            .arg("-c")
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: "jack_lsp -c".into(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(AudioError::CommandFailed {
                command: "jack_lsp -c".into(),
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (_, inputs) = self.parse_jack_lsp_output(&stdout);
        Ok(inputs)
    }

    fn connect(&self, source: &str, destination: &str) -> Result<()> {
        let output = Command::new("jack_connect")
            .args([source, destination])
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: format!("jack_connect {} {}", source, destination),
                reason: e.to_string(),
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already connected" is not an error
        if stderr.contains("already") {
            return Ok(());
        }

        Err(AudioError::ConnectionFailed {
            source: source.into(),
            destination: destination.into(),
            reason: stderr.into_owned(),
        })
    }

    fn disconnect(&self, source: &str, destination: &str) -> Result<()> {
        let output = Command::new("jack_disconnect")
            .args([source, destination])
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: format!("jack_disconnect {} {}", source, destination),
                reason: e.to_string(),
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AudioError::DisconnectionFailed {
            source: source.into(),
            destination: destination.into(),
            reason: stderr.into_owned(),
        })
    }

    fn list_connections(&self, port: &str) -> Result<Vec<String>> {
        let output = Command::new("jack_lsp")
            .arg("-c")
            .output()
            .map_err(|e| AudioError::CommandFailed {
                command: "jack_lsp -c".into(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut connections = Vec::new();
        let mut in_target_port = false;

        for line in stdout.lines() {
            if line.starts_with(' ') || line.starts_with('\t') {
                if in_target_port {
                    let trimmed = line.trim();
                    if trimmed.contains(':') {
                        connections.push(trimmed.to_string());
                    }
                }
            } else if line.trim() == port {
                in_target_port = true;
            } else {
                in_target_port = false;
            }
        }

        Ok(connections)
    }
}

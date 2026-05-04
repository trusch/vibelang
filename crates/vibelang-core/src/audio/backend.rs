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

    /// List every active port-to-port connection in one shot, as
    /// `(source, destination)` pairs.
    ///
    /// Backends that can't enumerate the global graph in a single call
    /// should leave the default impl in place; callers must fall back
    /// to per-port `list_connections` queries when this returns empty.
    fn list_all_connections(&self) -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }
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
        let output =
            Command::new("pw-link")
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
        let output =
            Command::new("pw-link")
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

    fn list_all_connections(&self) -> Result<Vec<(String, String)>> {
        let output =
            Command::new("pw-link")
                .arg("-l")
                .output()
                .map_err(|e| AudioError::CommandFailed {
                    command: "pw-link -l".into(),
                    reason: e.to_string(),
                })?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_pw_link_l(&stdout))
    }
}

/// Parse `pw-link -l` output into `(source, destination)` pairs.
///
/// Output format (verified on PipeWire 1.x): each port that participates
/// in any link gets a header line at column 0 (`node:port`), followed by
/// indented connection lines that begin with `|->` (egress; this header
/// is the source) or `|<-` (ingress; this header is the destination).
/// Each link therefore appears twice — once on each endpoint's block.
/// We consume only `|->` lines to emit each link exactly once.
///
/// Example:
/// ```text
/// some_source:port_a
///   |-> dest_node:port_x
///   |-> dest_node:port_y
/// dest_node:port_x
///   |<- some_source:port_a
/// ```
/// yields `[("some_source:port_a", "dest_node:port_x"),
///          ("some_source:port_a", "dest_node:port_y")]`.
fn parse_pw_link_l(output: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut current_header: Option<String> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let is_indented = line.starts_with(' ') || line.starts_with('\t');
        if !is_indented {
            current_header = Some(line.trim().to_string());
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("|->") {
            if let Some(src) = current_header.as_deref() {
                let dst = rest.trim();
                if !dst.is_empty() && dst.contains(':') {
                    pairs.push((src.to_string(), dst.to_string()));
                }
            }
        }
        // `|<-` lines describe the same link from the destination side; skip.
    }

    pairs
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
                    let mut port = Port::parse(
                        &port_name,
                        if is_output {
                            PortDirection::Output
                        } else {
                            PortDirection::Input
                        },
                    );
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
            let mut port = Port::parse(
                &port_name,
                if is_output {
                    PortDirection::Output
                } else {
                    PortDirection::Input
                },
            );
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
        let output =
            Command::new("jack_lsp")
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
        let output =
            Command::new("jack_lsp")
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
        let output =
            Command::new("jack_lsp")
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
            } else {
                in_target_port = line.trim() == port;
            }
        }

        Ok(connections)
    }

    fn list_all_connections(&self) -> Result<Vec<(String, String)>> {
        let output =
            Command::new("jack_lsp")
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
        let (outputs, _) = self.parse_jack_lsp_output(&stdout);
        // Emit pairs from the outputs side only — the same link is also
        // listed under its input port, so iterating only outputs gives
        // each link exactly once.
        let mut pairs = Vec::new();
        for port in outputs {
            for dst in port.connections {
                pairs.push((port.name.clone(), dst));
            }
        }
        Ok(pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_pw_link_l;

    #[test]
    fn parse_pw_link_l_extracts_egress_pairs() {
        // Captured fixture: header lines at column 0 are port names;
        // indented `|->` lines mean the header is the source; `|<-`
        // means the header is the destination (same link, listed twice).
        let fixture = "\
alsa_output.pci-0000_00_1f.3:playback_FL
  |<- Brave:output_FL
alsa_input.pci-0000_00_1f.3:capture_FL
  |-> Brave input:input_FL
  |-> Brave input:input_FL
Brave:output_FL
  |-> alsa_output.pci-0000_00_1f.3:playback_FL
Brave input:input_FL
  |<- alsa_input.pci-0000_00_1f.3:capture_FL
";
        let pairs = parse_pw_link_l(fixture);

        // Three `|->` edges in the fixture; each `|<-` is the same link
        // viewed from the destination side, so it must NOT yield a pair.
        assert_eq!(
            pairs,
            vec![
                (
                    "alsa_input.pci-0000_00_1f.3:capture_FL".to_string(),
                    "Brave input:input_FL".to_string(),
                ),
                (
                    "alsa_input.pci-0000_00_1f.3:capture_FL".to_string(),
                    "Brave input:input_FL".to_string(),
                ),
                (
                    "Brave:output_FL".to_string(),
                    "alsa_output.pci-0000_00_1f.3:playback_FL".to_string(),
                ),
            ],
        );
    }

    #[test]
    fn parse_pw_link_l_handles_empty_output() {
        assert!(parse_pw_link_l("").is_empty());
    }

    #[test]
    fn parse_pw_link_l_handles_indented_without_header() {
        // Defensive: indented line before any header should be ignored,
        // not panic / not produce a malformed pair.
        let fixture = "  |-> some:port\nheader:port\n  |-> dest:port\n";
        let pairs = parse_pw_link_l(fixture);
        assert_eq!(
            pairs,
            vec![("header:port".to_string(), "dest:port".to_string())],
        );
    }
}

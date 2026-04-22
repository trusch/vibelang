//! High-level audio connection manager.

use super::backend::{AudioBackend, JackBackend, PipeWireBackend};
use super::matcher::PortMatcher;
use super::{AudioError, Port, PortDirection, Result};
use tracing::{debug, info, warn};

/// High-level audio connector with fuzzy matching support.
///
/// This is the main entry point for managing audio connections.
/// It auto-detects available backends and provides a unified interface.
pub struct AudioConnector {
    /// Available backends in priority order (first working one is used)
    backends: Vec<Box<dyn AudioBackend>>,
}

impl AudioConnector {
    /// Create a new connector by detecting available backends.
    pub fn detect() -> Result<Self> {
        let mut backends: Vec<Box<dyn AudioBackend>> = Vec::new();
        let mut tried = Vec::new();

        // Try PipeWire first (more common on modern Linux)
        tried.push("pw-link".to_string());
        if let Some(pw) = PipeWireBackend::new() {
            info!("Detected PipeWire backend (pw-link)");
            backends.push(Box::new(pw));
        }

        // Try JACK
        tried.push("jack_lsp".to_string());
        if let Some(jack) = JackBackend::new() {
            info!("Detected JACK backend (jack_lsp/jack_connect)");
            backends.push(Box::new(jack));
        }

        if backends.is_empty() {
            return Err(AudioError::NoBackendAvailable { tried });
        }

        Ok(Self { backends })
    }

    /// Get the primary backend name.
    pub fn backend_name(&self) -> &str {
        self.backends.first().map(|b| b.name()).unwrap_or("none")
    }

    /// List all output ports (audio sources).
    pub fn list_output_ports(&self) -> Result<Vec<Port>> {
        for backend in &self.backends {
            match backend.list_output_ports() {
                Ok(ports) => return Ok(ports),
                Err(e) => {
                    debug!("Backend {} failed to list outputs: {}", backend.name(), e);
                }
            }
        }
        Ok(Vec::new())
    }

    /// List all input ports (audio sinks).
    pub fn list_input_ports(&self) -> Result<Vec<Port>> {
        for backend in &self.backends {
            match backend.list_input_ports() {
                Ok(ports) => return Ok(ports),
                Err(e) => {
                    debug!("Backend {} failed to list inputs: {}", backend.name(), e);
                }
            }
        }
        Ok(Vec::new())
    }

    /// Find ports matching a pattern.
    ///
    /// Returns all ports that match the pattern, sorted by match quality.
    pub fn find_ports(&self, pattern: &str, direction: PortDirection) -> Result<Vec<Port>> {
        let ports = match direction {
            PortDirection::Output => self.list_output_ports()?,
            PortDirection::Input => self.list_input_ports()?,
        };

        let port_names: Vec<String> = ports.iter().map(|p| p.name.clone()).collect();
        let matches = PortMatcher::find_matches(&port_names, pattern);

        Ok(matches
            .into_iter()
            .filter_map(|(name, _)| ports.iter().find(|p| p.name == name).cloned())
            .collect())
    }

    /// Resolve a port pattern to an exact port name.
    ///
    /// If the pattern matches exactly one port, returns it.
    /// If it matches multiple ports, returns an AmbiguousMatch error.
    /// If it matches no ports, returns a PortNotFound error with suggestions.
    pub fn resolve_port(&self, pattern: &str, direction: PortDirection) -> Result<String> {
        let ports = match direction {
            PortDirection::Output => self.list_output_ports()?,
            PortDirection::Input => self.list_input_ports()?,
        };

        let port_names: Vec<String> = ports.iter().map(|p| p.name.clone()).collect();

        // First, check for exact match
        if port_names.iter().any(|p| p == pattern) {
            return Ok(pattern.to_string());
        }

        // Find matching ports
        let matches: Vec<String> = ports
            .iter()
            .filter(|p| p.matches(pattern))
            .map(|p| p.name.clone())
            .collect();

        match matches.len() {
            0 => {
                // No match - provide suggestions
                let suggestions = PortMatcher::find_matches(&port_names, pattern)
                    .into_iter()
                    .take(5)
                    .map(|(name, _)| name.to_string())
                    .collect();

                Err(AudioError::PortNotFound {
                    pattern: pattern.to_string(),
                    suggestions,
                })
            }
            1 => {
                // Safety: len() == 1, so into_iter().next() is guaranteed to be Some
                #[allow(clippy::unwrap_used)]
                let name = matches.into_iter().next().unwrap();
                Ok(name)
            }
            _ => Err(AudioError::AmbiguousMatch {
                pattern: pattern.to_string(),
                matches,
            }),
        }
    }

    /// Connect two ports with fuzzy matching support.
    ///
    /// Both source and destination can be patterns that will be resolved
    /// to exact port names.
    pub fn connect(&self, source: &str, destination: &str) -> Result<()> {
        // Resolve source (must be an output port)
        let resolved_source = self.resolve_port(source, PortDirection::Output)?;

        // Resolve destination (must be an input port)
        let resolved_dest = self.resolve_port(destination, PortDirection::Input)?;

        info!("Connecting {} -> {}", resolved_source, resolved_dest);

        // Try each backend until one succeeds
        for backend in &self.backends {
            match backend.connect(&resolved_source, &resolved_dest) {
                Ok(()) => {
                    debug!("Connected via {}", backend.name());
                    return Ok(());
                }
                Err(e) => {
                    debug!("Backend {} failed to connect: {}", backend.name(), e);
                }
            }
        }

        Err(AudioError::ConnectionFailed {
            source: resolved_source,
            destination: resolved_dest,
            reason: "All backends failed".into(),
        })
    }

    /// Disconnect two ports with fuzzy matching support.
    pub fn disconnect(&self, source: &str, destination: &str) -> Result<()> {
        // Resolve source (must be an output port)
        let resolved_source = self.resolve_port(source, PortDirection::Output)?;

        // Resolve destination (must be an input port)
        let resolved_dest = self.resolve_port(destination, PortDirection::Input)?;

        info!("Disconnecting {} -> {}", resolved_source, resolved_dest);

        // Try each backend until one succeeds
        for backend in &self.backends {
            match backend.disconnect(&resolved_source, &resolved_dest) {
                Ok(()) => {
                    debug!("Disconnected via {}", backend.name());
                    return Ok(());
                }
                Err(e) => {
                    debug!("Backend {} failed to disconnect: {}", backend.name(), e);
                }
            }
        }

        Err(AudioError::DisconnectionFailed {
            source: resolved_source,
            destination: resolved_dest,
            reason: "All backends failed".into(),
        })
    }

    /// Disconnect all connections from/to a client.
    pub fn disconnect_client(&self, client_name: &str) -> Result<()> {
        info!("Disconnecting all ports for client: {}", client_name);

        // Get all ports for this client
        let outputs = self.list_output_ports()?;
        let inputs = self.list_input_ports()?;

        let client_outputs: Vec<_> = outputs
            .iter()
            .filter(|p| p.client.contains(client_name) || p.name.starts_with(client_name))
            .collect();

        let client_inputs: Vec<_> = inputs
            .iter()
            .filter(|p| p.client.contains(client_name) || p.name.starts_with(client_name))
            .collect();

        // Disconnect all output connections
        for port in client_outputs {
            if let Some(backend) = self.backends.first() {
                if let Ok(connections) = backend.list_connections(&port.name) {
                    for conn in connections {
                        let _ = backend.disconnect(&port.name, &conn);
                    }
                }
            }
        }

        // Disconnect all input connections
        for port in client_inputs {
            if let Some(backend) = self.backends.first() {
                // For inputs, we need to find what's connected TO them
                // This is trickier - we'd need to scan all outputs
                let all_outputs = backend.list_output_ports().unwrap_or_default();
                for out_port in all_outputs {
                    if let Ok(connections) = backend.list_connections(&out_port.name) {
                        if connections.iter().any(|c| c == &port.name) {
                            let _ = backend.disconnect(&out_port.name, &port.name);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Connect a JACK client to specific ports with pattern matching.
    ///
    /// This is a convenience method for setting up SuperCollider/scsynth connections.
    ///
    /// # Arguments
    /// * `client_name` - The JACK client name (e.g., "Vibelang")
    /// * `output_patterns` - Patterns for output connections (one per output channel)
    /// * `input_patterns` - Patterns for input connections (one per input channel)
    pub fn connect_client(
        &self,
        client_name: &str,
        output_patterns: &[String],
        input_patterns: &[String],
    ) -> Result<()> {
        // Connect outputs
        for (i, pattern) in output_patterns.iter().enumerate() {
            let src = format!("{}:out_{}", client_name, i + 1);
            match self.connect(&src, pattern) {
                Ok(()) => {
                    info!("Connected {} -> {}", src, pattern);
                }
                Err(e) => {
                    warn!("Failed to connect {} -> {}: {}", src, pattern, e);
                    // Continue with other connections
                }
            }
        }

        // Connect inputs
        for (i, pattern) in input_patterns.iter().enumerate() {
            let dst = format!("{}:in_{}", client_name, i + 1);
            match self.connect(pattern, &dst) {
                Ok(()) => {
                    info!("Connected {} -> {}", pattern, dst);
                }
                Err(e) => {
                    warn!("Failed to connect {} -> {}: {}", pattern, dst, e);
                    // Continue with other connections
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backends() {
        // This test may fail in environments without pw-link or jack_lsp
        let result = AudioConnector::detect();
        // We don't assert success since it depends on the environment
        println!(
            "Backend detection result: {:?}",
            result.map(|c| c.backend_name().to_string())
        );
    }
}

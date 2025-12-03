//! OSC (Open Sound Control) client for SuperCollider communication.
//!
//! OSC is the protocol used by SuperCollider's synthesis server (scsynth)
//! for real-time control. This module provides a simple UDP-based client.

use anyhow::Result;
use rosc::{encoder, OscBundle, OscMessage, OscPacket, OscTime, OscType};
use std::net::UdpSocket;
use std::sync::Arc;

/// UDP-based OSC client for sending messages to scsynth.
#[derive(Clone)]
pub struct OscClient {
    /// The underlying UDP socket.
    pub sock: Arc<UdpSocket>,
    /// Target address in "host:port" format.
    pub addr: String,
}

impl OscClient {
    /// Create a new OSC client targeting the given address.
    ///
    /// # Arguments
    /// * `addr` - The target address in "host:port" format (e.g., "127.0.0.1:57110")
    ///
    /// # Returns
    /// A new client bound to an ephemeral port.
    pub fn new<A: Into<String>>(addr: A) -> Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self {
            sock: Arc::new(sock),
            addr: addr.into(),
        })
    }

    /// Send an OSC message with the given path and arguments.
    ///
    /// # Arguments
    /// * `path` - The OSC address pattern (e.g., "/s_new", "/n_set")
    /// * `args` - The message arguments
    pub fn send_msg(&self, path: &str, args: Vec<OscType>) -> Result<()> {
        let msg = OscMessage {
            addr: path.into(),
            args,
        };
        let packet = OscPacket::Message(msg);
        let buf = encoder::encode(&packet)?;
        self.sock.send_to(&buf, &self.addr)?;
        Ok(())
    }

    /// Send an OSC bundle with a timetag for scheduled execution.
    ///
    /// # Arguments
    /// * `timetag` - Optional NTP timestamp for scheduling (None = immediately)
    /// * `packets` - The messages/bundles to include
    pub fn send_bundle(&self, timetag: Option<OscTime>, packets: Vec<OscPacket>) -> Result<()> {
        let bundle = OscBundle {
            timetag: timetag.unwrap_or_else(|| OscTime::from((1, 0))),
            content: packets,
        };
        let buf = encoder::encode(&OscPacket::Bundle(bundle))?;
        self.sock.send_to(&buf, &self.addr)?;
        Ok(())
    }

    /// Create an OSC message packet (for use in bundles).
    pub fn msg(path: &str, args: Vec<OscType>) -> OscPacket {
        OscPacket::Message(OscMessage {
            addr: path.into(),
            args,
        })
    }

    /// Receive an OSC message (blocking).
    ///
    /// # Returns
    /// The decoded OSC packet, or an error.
    pub fn recv_msg(&self) -> Result<OscPacket> {
        let mut buf = [0u8; 65536];
        let (size, _) = self.sock.recv_from(&mut buf)?;
        let (_, packet) = rosc::decoder::decode_udp(&buf[..size])?;
        Ok(packet)
    }

    /// Try to receive an OSC message without blocking.
    ///
    /// # Returns
    /// `Ok(Some(packet))` if a message is available,
    /// `Ok(None)` if no message is available,
    /// or an error if receiving/parsing fails.
    pub fn try_recv_msg(&self) -> Result<Option<OscPacket>> {
        self.sock.set_nonblocking(true)?;
        let mut buf = [0u8; 65536];
        let result = match self.sock.recv_from(&mut buf) {
            Ok((size, _)) => match rosc::decoder::decode_udp(&buf[..size]) {
                Ok((_, packet)) => Ok(Some(packet)),
                Err(e) => Err(anyhow::anyhow!("Failed to decode OSC packet: {}", e)),
            },
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("Failed to receive OSC message: {}", e))
                }
            }
        };
        let _ = self.sock.set_nonblocking(false);
        result
    }
}

impl std::fmt::Debug for OscClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OscClient")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osc_client_creation() {
        // Just test that we can create a client (won't actually connect)
        let client = OscClient::new("127.0.0.1:57110");
        assert!(client.is_ok());
    }

    #[test]
    fn test_msg_helper() {
        use rosc::OscType;
        let packet = OscClient::msg("/test", vec![OscType::Int(42), OscType::Float(3.14)]);
        if let OscPacket::Message(msg) = packet {
            assert_eq!(msg.addr, "/test");
            assert_eq!(msg.args.len(), 2);
        } else {
            panic!("Expected message packet");
        }
    }
}

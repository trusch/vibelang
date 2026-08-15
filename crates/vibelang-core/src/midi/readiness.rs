use crate::types::MidiDeviceId;
use serde::{Deserialize, Serialize};

const MIDI_INPUT_INTENT_FLAG: u32 = 0x2000_0000;
const MIDI_INPUT_INTENT_PAYLOAD: u32 = 0x1fff_ffff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiInputIntent {
    pub device_id: MidiDeviceId,
    pub role: String,
    pub exact_client: String,
}

impl MidiInputIntent {
    pub fn new(role: impl Into<String>, exact_client: impl Into<String>) -> Self {
        let role = role.into().trim().to_string();
        let exact_client = exact_client.into().trim().to_string();
        Self {
            device_id: midi_input_intent_id(&role, &exact_client),
            role,
            exact_client,
        }
    }
}

pub fn midi_input_intent_id(role: &str, exact_client: &str) -> MidiDeviceId {
    let mut hash = 0x811c_9dc5u32;
    for byte in role
        .trim()
        .to_lowercase()
        .bytes()
        .chain(std::iter::once(0))
        .chain(exact_client.trim().to_lowercase().bytes())
    {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    MidiDeviceId::new(MIDI_INPUT_INTENT_FLAG | (hash & MIDI_INPUT_INTENT_PAYLOAD))
}

pub fn is_midi_input_intent_id(id: MidiDeviceId) -> bool {
    id.raw() & 0xe000_0000 == MIDI_INPUT_INTENT_FLAG
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MidiReadinessState {
    #[default]
    Waiting,
    Connected,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MidiEndpointReadiness {
    pub role: String,
    pub state: MidiReadinessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct MidiReadiness {
    pub primary_role: String,
    pub endpoints: Vec<MidiEndpointReadiness>,
}

impl MidiReadiness {
    pub(crate) fn from_endpoints(mut endpoints: Vec<MidiEndpointReadiness>) -> Self {
        endpoints.sort_by_key(|endpoint| readiness_role_order(&endpoint.role));
        let primary_role = endpoints
            .iter()
            .find(|endpoint| endpoint.role.eq_ignore_ascii_case("gamma"))
            .or_else(|| endpoints.first())
            .map(|endpoint| endpoint.role.clone())
            .unwrap_or_default();
        Self {
            primary_role,
            endpoints,
        }
    }
}

fn readiness_role_order(role: &str) -> u8 {
    if role.eq_ignore_ascii_case("gamma") {
        0
    } else if role.eq_ignore_ascii_case("f_midi") {
        1
    } else if role.eq_ignore_ascii_case("panel") {
        2
    } else {
        3
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyMidiPort {
    pub id: MidiDeviceId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LegacyMidiBinding {
    port: LegacyMidiPort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LegacyInputAction {
    None,
    Disconnect,
    Open {
        port: LegacyMidiPort,
        disconnect_first: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidiInputIntentRuntime {
    pub intent: MidiInputIntent,
    binding: Option<LegacyMidiBinding>,
    readiness: MidiEndpointReadiness,
}

impl MidiInputIntentRuntime {
    pub(crate) fn new(intent: MidiInputIntent) -> Self {
        let readiness = waiting_readiness(&intent);
        Self {
            intent,
            binding: None,
            readiness,
        }
    }

    pub(crate) fn observe(
        &mut self,
        discovery: Result<&[LegacyMidiPort], &str>,
    ) -> LegacyInputAction {
        let ports = match discovery {
            Ok(ports) => ports,
            Err(error) => {
                let disconnect = self.binding.take().is_some();
                self.readiness = MidiEndpointReadiness {
                    role: self.intent.role.clone(),
                    state: MidiReadinessState::Unavailable,
                    resolved_name: None,
                    detail: Some(format!(
                        "MIDI input discovery failed for role '{}': {}",
                        self.intent.role, error
                    )),
                };
                return if disconnect {
                    LegacyInputAction::Disconnect
                } else {
                    LegacyInputAction::None
                };
            }
        };

        let matches: Vec<_> = ports
            .iter()
            .filter(|port| {
                alsa_client_name(&port.name).to_lowercase().eq(&self
                    .intent
                    .exact_client
                    .trim()
                    .to_lowercase())
            })
            .cloned()
            .collect();

        match matches.as_slice() {
            [] => {
                let disconnect = self.binding.take().is_some();
                self.readiness = waiting_readiness(&self.intent);
                if disconnect {
                    LegacyInputAction::Disconnect
                } else {
                    LegacyInputAction::None
                }
            }
            [port] => {
                if self
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.port == *port)
                {
                    self.readiness = connected_readiness(&self.intent, port);
                    LegacyInputAction::None
                } else {
                    let disconnect_first = self.binding.take().is_some();
                    self.readiness = waiting_readiness(&self.intent);
                    LegacyInputAction::Open {
                        port: port.clone(),
                        disconnect_first,
                    }
                }
            }
            _ => {
                let disconnect = self.binding.take().is_some();
                let names = matches
                    .iter()
                    .map(|port| port.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.readiness = MidiEndpointReadiness {
                    role: self.intent.role.clone(),
                    state: MidiReadinessState::Unavailable,
                    resolved_name: None,
                    detail: Some(format!(
                        "Multiple MIDI inputs match exact ALSA client '{}': {}",
                        self.intent.exact_client, names
                    )),
                };
                if disconnect {
                    LegacyInputAction::Disconnect
                } else {
                    LegacyInputAction::None
                }
            }
        }
    }

    pub(crate) fn mark_opened(&mut self, port: LegacyMidiPort) {
        self.readiness = connected_readiness(&self.intent, &port);
        self.binding = Some(LegacyMidiBinding { port });
    }

    pub(crate) fn mark_open_failed(&mut self, port: &LegacyMidiPort, error: &str) {
        self.binding = None;
        self.readiness = MidiEndpointReadiness {
            role: self.intent.role.clone(),
            state: MidiReadinessState::Unavailable,
            resolved_name: Some(port.name.clone()),
            detail: Some(format!(
                "Failed to open MIDI input '{}' for role '{}': {}",
                port.name, self.intent.role, error
            )),
        };
    }

    pub(crate) fn readiness(&self) -> MidiEndpointReadiness {
        self.readiness.clone()
    }

    pub(crate) fn is_bound(&self) -> bool {
        self.binding.is_some()
    }
}

fn alsa_client_name(port_name: &str) -> &str {
    port_name.split(':').next().unwrap_or(port_name).trim()
}

fn waiting_readiness(intent: &MidiInputIntent) -> MidiEndpointReadiness {
    MidiEndpointReadiness {
        role: intent.role.clone(),
        state: MidiReadinessState::Waiting,
        resolved_name: None,
        detail: Some(format!(
            "Waiting for exact ALSA MIDI client '{}'",
            intent.exact_client
        )),
    }
}

fn connected_readiness(intent: &MidiInputIntent, port: &LegacyMidiPort) -> MidiEndpointReadiness {
    MidiEndpointReadiness {
        role: intent.role.clone(),
        state: MidiReadinessState::Connected,
        resolved_name: Some(port.name.clone()),
        detail: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(id: u32, name: &str) -> LegacyMidiPort {
        LegacyMidiPort {
            id: MidiDeviceId::new(id),
            name: name.to_string(),
        }
    }

    #[test]
    fn midi_legacy_hotplug_late_arrival_disconnect_and_reconnect() {
        let intent = MidiInputIntent::new("gamma", "gamma");
        let mut runtime = MidiInputIntentRuntime::new(intent);

        assert_eq!(runtime.observe(Ok(&[])), LegacyInputAction::None);
        assert_eq!(runtime.readiness.state, MidiReadinessState::Waiting);

        let first = port(3, "GaMmA:Gamma MIDI 1 20:0");
        assert_eq!(
            runtime.observe(Ok(std::slice::from_ref(&first))),
            LegacyInputAction::Open {
                port: first.clone(),
                disconnect_first: false,
            }
        );
        runtime.mark_opened(first.clone());
        assert_eq!(runtime.readiness.state, MidiReadinessState::Connected);

        assert_eq!(runtime.observe(Ok(&[])), LegacyInputAction::Disconnect);
        assert_eq!(runtime.readiness.state, MidiReadinessState::Waiting);

        let rebound = port(7, "gamma:Gamma MIDI 1 24:0");
        assert_eq!(
            runtime.observe(Ok(std::slice::from_ref(&rebound))),
            LegacyInputAction::Open {
                port: rebound.clone(),
                disconnect_first: false,
            }
        );
        runtime.mark_opened(rebound.clone());
        assert_eq!(runtime.readiness.state, MidiReadinessState::Connected);
        assert_eq!(runtime.readiness.resolved_name, Some(rebound.name));
    }

    #[test]
    fn midi_legacy_hotplug_exact_client_rejects_substrings_and_ambiguity() {
        let intent = MidiInputIntent::new("gamma", "gamma");
        let mut runtime = MidiInputIntentRuntime::new(intent);
        let substring = port(1, "super-gamma:Panel 20:0");

        assert_eq!(
            runtime.observe(Ok(std::slice::from_ref(&substring))),
            LegacyInputAction::None
        );
        assert_eq!(runtime.readiness.state, MidiReadinessState::Waiting);

        let duplicate = [port(2, "gamma:Keys 21:0"), port(3, "GAMMA:Pads 21:1")];
        assert_eq!(runtime.observe(Ok(&duplicate)), LegacyInputAction::None);
        assert_eq!(runtime.readiness.state, MidiReadinessState::Unavailable);
        assert!(runtime
            .readiness
            .detail
            .as_deref()
            .expect("ambiguous readiness should include detail")
            .contains("Multiple MIDI inputs"));
    }

    #[test]
    fn midi_legacy_hotplug_ambiguity_and_discovery_failure_disconnect_binding() {
        let intent = MidiInputIntent::new("gamma", "gamma");
        let mut runtime = MidiInputIntentRuntime::new(intent);
        let first = port(2, "gamma:Keys 21:0");

        assert!(matches!(
            runtime.observe(Ok(std::slice::from_ref(&first))),
            LegacyInputAction::Open { .. }
        ));
        runtime.mark_opened(first.clone());

        let duplicate = [first, port(3, "GAMMA:Pads 21:1")];
        assert_eq!(
            runtime.observe(Ok(&duplicate)),
            LegacyInputAction::Disconnect
        );
        assert_eq!(runtime.readiness.state, MidiReadinessState::Unavailable);

        let rebound = port(4, "gamma:Keys 22:0");
        assert!(matches!(
            runtime.observe(Ok(std::slice::from_ref(&rebound))),
            LegacyInputAction::Open {
                disconnect_first: false,
                ..
            }
        ));
        runtime.mark_opened(rebound);

        assert_eq!(
            runtime.observe(Err("ALSA sequencer unavailable")),
            LegacyInputAction::Disconnect
        );
        assert_eq!(runtime.readiness.state, MidiReadinessState::Unavailable);
        assert!(runtime
            .readiness
            .detail
            .as_deref()
            .expect("discovery failure readiness should include detail")
            .contains("ALSA sequencer unavailable"));
    }

    #[test]
    fn midi_legacy_hotplug_open_failure_retries_without_prior_numeric_id() {
        let intent = MidiInputIntent::new("gamma", "gamma");
        let logical_id = intent.device_id;
        let mut runtime = MidiInputIntentRuntime::new(intent);
        let first = port(4, "gamma:Keys 22:0");

        let LegacyInputAction::Open {
            port: selected_port,
            ..
        } = runtime.observe(Ok(std::slice::from_ref(&first)))
        else {
            panic!("present exact client should request an open");
        };
        runtime.mark_open_failed(&selected_port, "permission denied");
        assert_eq!(runtime.readiness.state, MidiReadinessState::Unavailable);
        assert!(runtime
            .readiness
            .detail
            .as_deref()
            .expect("open failure readiness should include detail")
            .contains("permission denied"));

        let rebound = port(9, "gamma:Keys 29:0");
        assert!(matches!(
            runtime.observe(Ok(std::slice::from_ref(&rebound))),
            LegacyInputAction::Open { ref port, .. } if port.id == MidiDeviceId::new(9)
        ));
        runtime.mark_opened(rebound);
        assert_eq!(runtime.intent.device_id, logical_id);
        assert_eq!(runtime.readiness.state, MidiReadinessState::Connected);
    }

    #[test]
    fn midi_input_intent_id_is_normalized_and_reserved() {
        let canonical = MidiInputIntent::new("gamma", "gamma");
        let padded = MidiInputIntent::new(" Gamma ", " GAMMA ");

        assert_eq!(canonical.device_id, padded.device_id);
        assert_eq!(padded.role, "Gamma");
        assert_eq!(padded.exact_client, "GAMMA");
        assert!(is_midi_input_intent_id(canonical.device_id));
        assert!(!is_midi_input_intent_id(MidiDeviceId::new(3)));
        assert!(!is_midi_input_intent_id(MidiDeviceId::new(0x4000_0000)));
        assert!(!is_midi_input_intent_id(MidiDeviceId::new(0x8000_0000)));
    }

    #[test]
    fn midi_input_intent_readiness_has_fixed_role_order_and_gamma_primary() {
        let endpoint = |role: &str| MidiEndpointReadiness {
            role: role.to_string(),
            state: MidiReadinessState::Waiting,
            resolved_name: None,
            detail: None,
        };

        let readiness = MidiReadiness::from_endpoints(vec![
            endpoint("panel"),
            endpoint("aux"),
            endpoint("F_MIDI"),
            endpoint("Gamma"),
        ]);

        assert_eq!(readiness.primary_role, "Gamma");
        assert_eq!(
            readiness
                .endpoints
                .iter()
                .map(|endpoint| endpoint.role.as_str())
                .collect::<Vec<_>>(),
            ["Gamma", "F_MIDI", "panel", "aux"]
        );
    }
}

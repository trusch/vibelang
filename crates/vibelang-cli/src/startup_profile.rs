use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use vibelang_core::audio::PortMatcher;

const DEFAULT_AUDIO_CLIENT: &str = "SuperCollider";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupProfile {
    pub version: u32,
    pub name: String,
    pub audio: AudioProfile,
    #[serde(default, rename = "service")]
    pub services: Vec<ServiceRequirement>,
    #[serde(default, rename = "endpoint")]
    pub endpoints: Vec<EndpointRequirement>,
    #[serde(default)]
    pub policy: StartupPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioProfile {
    pub input_channels: u32,
    pub output_channels: u32,
    #[serde(default = "default_audio_client")]
    pub client: String,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub manage_links: bool,
    #[serde(default, rename = "input")]
    pub inputs: Vec<AudioLink>,
    #[serde(default, rename = "output")]
    pub outputs: Vec<AudioLink>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioLink {
    pub channel: u32,
    pub name: String,
    pub external_port: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRequirement {
    pub name: String,
    pub unit: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointRequirement {
    pub name: String,
    pub pattern: String,
    #[serde(default)]
    pub backend: EndpointBackend,
    #[serde(default)]
    pub direction: EndpointDirection,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointBackend {
    #[default]
    Pipewire,
    Midi,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointDirection {
    Source,
    Sink,
    #[default]
    Any,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupPolicy {
    #[serde(default)]
    pub allow_degraded_start: bool,
    #[serde(default = "default_readiness_timeout_ms")]
    pub readiness_timeout_ms: u64,
}

impl Default for StartupPolicy {
    fn default() -> Self {
        Self {
            allow_degraded_start: false,
            readiness_timeout_ms: default_readiness_timeout_ms(),
        }
    }
}

fn default_audio_client() -> String {
    DEFAULT_AUDIO_CLIENT.to_string()
}

fn default_true() -> bool {
    true
}

fn default_readiness_timeout_ms() -> u64 {
    2_000
}

impl StartupProfile {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read profile {}", path.display()))?;
        Self::parse(&contents)
            .with_context(|| format!("invalid startup profile {}", path.display()))
    }

    pub fn resolve_path(
        script: &Path,
        explicit: Option<&Path>,
    ) -> Result<Option<std::path::PathBuf>> {
        if let Some(path) = explicit {
            return Ok(Some(path.to_path_buf()));
        }

        let contents = std::fs::read_to_string(script)
            .with_context(|| format!("failed to read script {}", script.display()))?;
        for line in contents.lines().take(32) {
            let Some(value) = line.trim().strip_prefix("// vibe-profile:") else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                bail!("empty // vibe-profile: directive in {}", script.display());
            }
            let path = Path::new(value);
            return Ok(Some(if path.is_absolute() {
                path.to_path_buf()
            } else {
                script.parent().unwrap_or_else(|| Path::new(".")).join(path)
            }));
        }

        Ok(None)
    }

    fn parse(contents: &str) -> Result<Self> {
        let profile: Self = toml::from_str(contents).context("failed to parse TOML")?;
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported profile version {}; expected 1", self.version);
        }
        if self.name.trim().is_empty() {
            bail!("profile name must not be empty");
        }
        if self.audio.client.trim().is_empty() {
            bail!("audio.client must not be empty");
        }
        if self
            .audio
            .device
            .as_deref()
            .is_some_and(|device| device.trim().is_empty())
        {
            bail!("audio.device must not be empty when set");
        }
        if !(1..=60_000).contains(&self.policy.readiness_timeout_ms) {
            bail!("policy.readiness_timeout_ms must be in 1..=60000");
        }
        validate_audio_links("input", self.audio.input_channels, &self.audio.inputs)?;
        validate_audio_links("output", self.audio.output_channels, &self.audio.outputs)?;
        validate_named_requirements(
            "service",
            self.services
                .iter()
                .map(|service| (service.name.as_str(), service.unit.as_str(), "unit")),
        )?;
        validate_named_requirements(
            "endpoint",
            self.endpoints
                .iter()
                .map(|endpoint| (endpoint.name.as_str(), endpoint.pattern.as_str(), "pattern")),
        )?;
        Ok(())
    }

    pub fn resolve_channel_counts(
        &self,
        requested_inputs: Option<u32>,
        requested_outputs: Option<u32>,
    ) -> Result<(u32, u32)> {
        if let Some(actual) = requested_inputs {
            if actual != self.audio.input_channels {
                bail!(
                    "FAILED profile '{}' requires {} input channels, but --input-channels requested {}",
                    self.name,
                    self.audio.input_channels,
                    actual
                );
            }
        }
        if let Some(actual) = requested_outputs {
            if actual != self.audio.output_channels {
                bail!(
                    "FAILED profile '{}' requires {} output channels, but --output-channels requested {}",
                    self.name,
                    self.audio.output_channels,
                    actual
                );
            }
        }
        Ok((self.audio.input_channels, self.audio.output_channels))
    }

    pub fn resolve_device(&self, requested: Option<String>) -> Result<Option<String>> {
        match (&self.audio.device, requested) {
            (Some(expected), Some(actual)) if actual != *expected => bail!(
                "FAILED profile '{}' requires audio device '{}', but --device requested '{}'",
                self.name,
                expected,
                actual
            ),
            (Some(expected), _) => Ok(Some(expected.clone())),
            (None, requested) => Ok(requested),
        }
    }

    pub fn manages_links(&self) -> bool {
        self.audio.manage_links
    }

    pub fn input_sources(&self) -> Vec<String> {
        ordered_external_ports(self.audio.input_channels, &self.audio.inputs)
    }

    pub fn output_destinations(&self) -> Vec<String> {
        ordered_external_ports(self.audio.output_channels, &self.audio.outputs)
    }

    pub fn inactive_required_services(&self) -> Vec<String> {
        self.services
            .iter()
            .filter(|service| service.required && !service_is_active(&service.unit))
            .map(|service| {
                format!(
                    "service '{}' ({}) is not active",
                    service.name, service.unit
                )
            })
            .collect()
    }

    pub fn wait_for_readiness(&self) -> Result<ReadinessReport> {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(self.policy.readiness_timeout_ms);
        loop {
            let report = self.evaluate(&ReadinessSnapshot::probe(self)?);
            if report.allow_transport_start || std::time::Instant::now() >= deadline {
                return Ok(report);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    pub fn evaluate(&self, snapshot: &ReadinessSnapshot) -> ReadinessReport {
        let mut required_missing = Vec::new();
        let mut optional_missing = Vec::new();

        for service in &self.services {
            if !snapshot.service_active(&service.unit) {
                push_missing(
                    service.required,
                    format!(
                        "service '{}' ({}) is not active",
                        service.name, service.unit
                    ),
                    &mut required_missing,
                    &mut optional_missing,
                );
            }
        }

        for endpoint in &self.endpoints {
            let (sources, sinks) = match endpoint.backend {
                EndpointBackend::Pipewire => (&snapshot.source_ports, &snapshot.sink_ports),
                EndpointBackend::Midi => (&snapshot.midi_input_ports, &snapshot.midi_output_ports),
            };
            let found = match endpoint.direction {
                EndpointDirection::Source => sources
                    .iter()
                    .any(|port| PortMatcher::matches(port, &endpoint.pattern)),
                EndpointDirection::Sink => sinks
                    .iter()
                    .any(|port| PortMatcher::matches(port, &endpoint.pattern)),
                EndpointDirection::Any => sources
                    .iter()
                    .chain(sinks)
                    .any(|port| PortMatcher::matches(port, &endpoint.pattern)),
            };
            if !found {
                push_missing(
                    endpoint.required,
                    format!(
                        "endpoint '{}' ({:?} {:?} '{}') is missing",
                        endpoint.name, endpoint.backend, endpoint.direction, endpoint.pattern
                    ),
                    &mut required_missing,
                    &mut optional_missing,
                );
            }
        }

        for link in &self.audio.inputs {
            let destination = format!("{}:in_{}", self.audio.client, link.channel);
            evaluate_link(
                link,
                &link.external_port,
                &destination,
                &snapshot.links,
                &mut required_missing,
                &mut optional_missing,
            );
        }
        for link in &self.audio.outputs {
            let source = format!("{}:out_{}", self.audio.client, link.channel);
            evaluate_link(
                link,
                &source,
                &link.external_port,
                &snapshot.links,
                &mut required_missing,
                &mut optional_missing,
            );
        }

        let state = if !required_missing.is_empty() {
            ReadinessState::Waiting
        } else if optional_missing.is_empty() {
            ReadinessState::Ready
        } else if self.policy.allow_degraded_start {
            ReadinessState::Degraded
        } else {
            required_missing.push(
                "optional dependencies are unavailable and policy.allow_degraded_start is false"
                    .to_string(),
            );
            ReadinessState::Waiting
        };

        ReadinessReport {
            state,
            required_missing,
            optional_missing,
            allow_transport_start: matches!(
                state,
                ReadinessState::Ready | ReadinessState::Degraded
            ),
        }
    }

    pub fn format_mapping(&self, state: ReadinessState) -> String {
        let mut lines = vec![format!(
            "{} profile '{}' — {} inputs / {} outputs",
            state.as_str(),
            self.name,
            self.audio.input_channels,
            self.audio.output_channels
        )];
        lines.push("  inputs:".to_string());
        for link in sorted_links(&self.audio.inputs) {
            lines.push(format!(
                "    in_{:02}  {:<24} {} -> {}:in_{}",
                link.channel, link.name, link.external_port, self.audio.client, link.channel
            ));
        }
        lines.push("  outputs:".to_string());
        for link in sorted_links(&self.audio.outputs) {
            lines.push(format!(
                "    out_{:02} {:<24} {}:out_{} -> {}",
                link.channel, link.name, self.audio.client, link.channel, link.external_port
            ));
        }
        lines.join("\n")
    }
}

fn validate_audio_links(kind: &str, count: u32, links: &[AudioLink]) -> Result<()> {
    if count == 0 {
        bail!("audio.{kind}_channels must be greater than zero");
    }
    if links.len() != count as usize {
        bail!(
            "audio.{kind}_channels is {count}, but {} named {kind} links were declared",
            links.len()
        );
    }

    let mut seen_channels = HashSet::new();
    let mut seen_names = HashSet::new();
    for link in links {
        if !(1..=count).contains(&link.channel) {
            bail!(
                "audio.{kind} channel {} is outside the declared 1..={count} range",
                link.channel
            );
        }
        if !seen_channels.insert(link.channel) {
            bail!(
                "audio.{kind} channel {} is declared more than once",
                link.channel
            );
        }
        if link.name.trim().is_empty() || link.external_port.trim().is_empty() {
            bail!("audio.{kind} links require non-empty name and external_port");
        }
        if !seen_names.insert(link.name.as_str()) {
            bail!(
                "audio.{kind} link name '{}' is declared more than once",
                link.name
            );
        }
    }
    Ok(())
}

fn validate_named_requirements<'a>(
    kind: &str,
    requirements: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
) -> Result<()> {
    let mut names = HashSet::new();
    for (name, value, value_label) in requirements {
        if name.trim().is_empty() || value.trim().is_empty() {
            bail!("{kind} requirements need non-empty name and {value_label}");
        }
        if !names.insert(name) {
            bail!("{kind} name '{name}' is declared more than once");
        }
    }
    Ok(())
}

fn ordered_external_ports(count: u32, links: &[AudioLink]) -> Vec<String> {
    let mut ports = vec![String::new(); count as usize];
    for link in links {
        ports[(link.channel - 1) as usize] = link.external_port.clone();
    }
    ports
}

fn sorted_links(links: &[AudioLink]) -> Vec<&AudioLink> {
    let mut sorted = links.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|link| link.channel);
    sorted
}

fn evaluate_link(
    link: &AudioLink,
    expected_source: &str,
    expected_destination: &str,
    actual_links: &[(String, String)],
    required_missing: &mut Vec<String>,
    optional_missing: &mut Vec<String>,
) {
    let found = actual_links.iter().any(|(source, destination)| {
        PortMatcher::matches(source, expected_source)
            && PortMatcher::matches(destination, expected_destination)
    });
    if !found {
        push_missing(
            link.required,
            format!(
                "audio link '{}' is missing ({} -> {})",
                link.name, expected_source, expected_destination
            ),
            required_missing,
            optional_missing,
        );
    }
}

fn push_missing(
    required: bool,
    message: String,
    required_missing: &mut Vec<String>,
    optional_missing: &mut Vec<String>,
) {
    if required {
        required_missing.push(message);
    } else {
        optional_missing.push(message);
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReadinessSnapshot {
    pub services: HashMap<String, bool>,
    pub source_ports: Vec<String>,
    pub sink_ports: Vec<String>,
    pub midi_input_ports: Vec<String>,
    pub midi_output_ports: Vec<String>,
    pub links: Vec<(String, String)>,
}

impl ReadinessSnapshot {
    pub fn probe(profile: &StartupProfile) -> Result<Self> {
        let services = profile
            .services
            .iter()
            .map(|service| (service.unit.clone(), service_is_active(&service.unit)))
            .collect();
        let source_ports = command_lines("pw-link", &["-o"])?;
        let sink_ports = command_lines("pw-link", &["-i"])?;
        let link_output = command_output("pw-link", &["-l"])?;
        let links = parse_pw_links(&link_output);
        let (midi_input_ports, midi_output_ports) = list_midi_ports();
        Ok(Self {
            services,
            source_ports,
            sink_ports,
            midi_input_ports,
            midi_output_ports,
            links,
        })
    }

    fn service_active(&self, unit: &str) -> bool {
        self.services.get(unit).copied().unwrap_or(false)
    }
}

#[cfg(feature = "midi")]
fn list_midi_ports() -> (Vec<String>, Vec<String>) {
    use midir::{MidiInput, MidiOutput};

    let mut inputs = MidiInput::new("vibelang-profile-inputs")
        .ok()
        .map(|midi| {
            midi.ports()
                .iter()
                .filter_map(|port| midi.port_name(port).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    inputs.extend(
        vibelang_core::midi::list_pipewire_midi2_inputs()
            .into_iter()
            .map(|input| input.name),
    );
    let outputs = MidiOutput::new("vibelang-profile-outputs")
        .ok()
        .map(|midi| {
            midi.ports()
                .iter()
                .filter_map(|port| midi.port_name(port).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (inputs, outputs)
}

#[cfg(not(feature = "midi"))]
fn list_midi_ports() -> (Vec<String>, Vec<String>) {
    (Vec::new(), Vec::new())
}

fn service_is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_lines(command: &str, args: &[&str]) -> Result<Vec<String>> {
    Ok(command_output(command, args)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn command_output(command: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {} {}", command, args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{} {} failed: {}",
            command,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_pw_links(output: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let mut current_source = None;
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            current_source = Some(line.trim());
            continue;
        }
        if let (Some(source), Some(destination)) =
            (current_source, line.trim_start().strip_prefix("|->"))
        {
            let destination = destination.trim();
            if !destination.is_empty() {
                links.push((source.to_string(), destination.to_string()));
            }
        }
    }
    links
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessState {
    Ready,
    Degraded,
    Waiting,
}

impl ReadinessState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
            Self::Waiting => "WAITING",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadinessReport {
    pub state: ReadinessState,
    pub required_missing: Vec<String>,
    pub optional_missing: Vec<String>,
    pub allow_transport_start: bool,
}

impl ReadinessReport {
    pub fn format_status(&self, profile_name: &str) -> String {
        let mut lines = vec![format!(
            "{} profile '{}'",
            self.state.as_str(),
            profile_name
        )];
        for cause in &self.required_missing {
            lines.push(format!("  required: {cause}"));
        }
        for cause in &self.optional_missing {
            lines.push(format!("  optional: {cause}"));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"
version = 1
name = "buddy-test"

[audio]
input_channels = 2
output_channels = 2
client = "SuperCollider"
device = "SuperCollider"
manage_links = true

[[audio.input]]
channel = 1
name = "bass"
external_port = "Focusrite:capture_1"

[[audio.input]]
channel = 2
name = "rack"
external_port = "Focusrite:capture_2"

[[audio.output]]
channel = 1
name = "monitor-left"
external_port = "Focusrite:playback_1"

[[audio.output]]
channel = 2
name = "monitor-right"
external_port = "Focusrite:playback_2"

[[service]]
name = "PipeWire"
unit = "pipewire.service"

[[endpoint]]
name = "M10D"
pattern = "M10D:midi-out"
backend = "midi"
direction = "source"
required = false

[policy]
allow_degraded_start = true
readiness_timeout_ms = 100
"#;

    fn profile() -> StartupProfile {
        StartupProfile::parse(PROFILE).unwrap()
    }

    fn ready_snapshot() -> ReadinessSnapshot {
        ReadinessSnapshot {
            services: HashMap::from([("pipewire.service".to_string(), true)]),
            source_ports: vec![
                "Focusrite:capture_1".to_string(),
                "Focusrite:capture_2".to_string(),
                "SuperCollider:out_1".to_string(),
                "SuperCollider:out_2".to_string(),
                "M10D:midi-out".to_string(),
            ],
            sink_ports: vec![
                "SuperCollider:in_1".to_string(),
                "SuperCollider:in_2".to_string(),
                "Focusrite:playback_1".to_string(),
                "Focusrite:playback_2".to_string(),
            ],
            midi_input_ports: vec!["M10D:midi-out".to_string()],
            midi_output_ports: Vec::new(),
            links: vec![
                ("Focusrite:capture_1".into(), "SuperCollider:in_1".into()),
                ("Focusrite:capture_2".into(), "SuperCollider:in_2".into()),
                ("SuperCollider:out_1".into(), "Focusrite:playback_1".into()),
                ("SuperCollider:out_2".into(), "Focusrite:playback_2".into()),
            ],
        }
    }

    #[test]
    fn rejects_wrong_channel_counts_before_startup() {
        let error = profile()
            .resolve_channel_counts(Some(8), Some(2))
            .unwrap_err();
        assert!(error.to_string().contains("FAILED"));
        assert!(error.to_string().contains("requires 2 input channels"));
    }

    #[test]
    fn rejects_audio_device_that_conflicts_with_profile() {
        let error = profile()
            .resolve_device(Some("default".to_string()))
            .unwrap_err();
        assert!(error.to_string().contains("FAILED"));
        assert!(error
            .to_string()
            .contains("requires audio device 'SuperCollider'"));
    }

    #[test]
    fn missing_required_link_waits_and_blocks_transport() {
        let profile = profile();
        let mut snapshot = ready_snapshot();
        snapshot.links.pop();
        let report = profile.evaluate(&snapshot);
        assert_eq!(report.state, ReadinessState::Waiting);
        assert!(!report.allow_transport_start);
        assert!(report.required_missing[0].contains("monitor-right"));
    }

    #[test]
    fn correct_mapping_is_ready_and_prints_logical_names_once() {
        let profile = profile();
        let report = profile.evaluate(&ready_snapshot());
        assert_eq!(report.state, ReadinessState::Ready);
        assert!(report.allow_transport_start);
        let mapping = profile.format_mapping(report.state);
        assert_eq!(mapping.matches("profile 'buddy-test'").count(), 1);
        assert!(mapping.contains("bass"));
        assert!(mapping.contains("monitor-right"));
    }

    #[test]
    fn optional_loss_is_degraded_only_when_policy_allows_start() {
        let mut snapshot = ready_snapshot();
        snapshot
            .midi_input_ports
            .retain(|port| !port.contains("M10D"));

        let allowed = profile().evaluate(&snapshot);
        assert_eq!(allowed.state, ReadinessState::Degraded);
        assert!(allowed.allow_transport_start);

        let mut denied_profile = profile();
        denied_profile.policy.allow_degraded_start = false;
        let denied = denied_profile.evaluate(&snapshot);
        assert_eq!(denied.state, ReadinessState::Waiting);
        assert!(!denied.allow_transport_start);
    }

    #[test]
    fn parses_each_pipewire_link_once_from_source_blocks() {
        let links =
            parse_pw_links("Source:out_1\n  |-> Sink:in_1\nSink:in_1\n  |<- Source:out_1\n");
        assert_eq!(links, vec![("Source:out_1".into(), "Sink:in_1".into())]);
    }

    #[test]
    fn profile_requires_one_named_link_per_channel() {
        let invalid = PROFILE.replace("output_channels = 2", "output_channels = 3");
        let error = StartupProfile::parse(&invalid).unwrap_err();
        assert!(error.to_string().contains("3"));
        assert!(error.to_string().contains("2 named output links"));
    }

    #[test]
    fn script_directive_resolves_profile_relative_to_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("studio.vibe");
        std::fs::write(
            &script,
            "// studio\n// vibe-profile: buddy-live.toml\nset_tempo(130);\n",
        )
        .unwrap();

        let resolved = StartupProfile::resolve_path(&script, None)
            .unwrap()
            .unwrap();
        assert_eq!(resolved, dir.path().join("buddy-live.toml"));
    }

    #[test]
    fn explicit_profile_overrides_script_directive() {
        let explicit = Path::new("operator.toml");
        let resolved = StartupProfile::resolve_path(Path::new("missing.vibe"), Some(explicit))
            .unwrap()
            .unwrap();
        assert_eq!(resolved, explicit);
    }
}

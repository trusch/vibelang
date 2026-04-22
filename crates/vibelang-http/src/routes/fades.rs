//! Fade endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use vibelang_core::{
    traits::{FadeConfig, FadeCurve, FadeTarget},
    types::Duration,
    EffectId, FadeMessage, GroupId, MelodyId, PatternId, VoiceId,
};

use crate::{
    models::{ErrorResponse, FadeCreate, FadeTargetType},
    AppState,
};

/// Curve specification that can be a simple string name or a complex curve definition.
///
/// # Examples (JSON)
/// ```json
/// // Simple string curve
/// {"curve": "ease_in_out"}
///
/// // Exponential with custom exponent
/// {"curve": {"exp": 3.0}}
///
/// // Cubic spline with control points
/// {"curve": {"spline": [[0.25, 0.1], [0.5, 0.9], [0.75, 0.3]]}}
/// ```
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum CurveSpec {
    /// Simple curve name (backward compatible).
    Name(String),
    /// Exponential curve with custom exponent.
    Exponential { exp: f64 },
    /// Cubic spline with control points.
    Spline { spline: Vec<[f64; 2]> },
}

impl Default for CurveSpec {
    fn default() -> Self {
        CurveSpec::Name("linear".to_string())
    }
}

/// Request body for starting a fade on a voice.
#[derive(Debug, Deserialize)]
pub struct VoiceFadeRequest {
    /// Parameter name to fade.
    pub param: String,
    /// Target value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
    /// Optional starting value (defaults to current value).
    #[serde(default)]
    pub from: Option<f32>,
    /// Interpolation curve specification.
    #[serde(default)]
    pub curve: CurveSpec,
}

/// Request body for starting a fade on a group.
#[derive(Debug, Deserialize)]
pub struct GroupFadeRequest {
    /// Parameter name to fade.
    pub param: String,
    /// Target value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
    /// Optional starting value (defaults to current value).
    #[serde(default)]
    pub from: Option<f32>,
    /// Interpolation curve specification.
    #[serde(default)]
    pub curve: CurveSpec,
}

/// Request body for starting a fade on an effect.
#[derive(Debug, Deserialize)]
pub struct EffectFadeRequest {
    /// Parameter name to fade.
    pub param: String,
    /// Target value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
    /// Optional starting value (defaults to current value).
    #[serde(default)]
    pub from: Option<f32>,
    /// Interpolation curve specification.
    #[serde(default)]
    pub curve: CurveSpec,
}

/// Parse a curve name string into a FadeCurve.
fn parse_curve_name(curve: &str) -> FadeCurve {
    match curve.to_lowercase().as_str() {
        // Ease (quadratic)
        "ease_in" | "easein" | "ease-in" => FadeCurve::EaseIn,
        "ease_out" | "easeout" | "ease-out" => FadeCurve::EaseOut,
        "ease_in_out" | "easeinout" | "ease-in-out" | "ease" => FadeCurve::EaseInOut,

        // Sine
        "sine_in" | "sinein" | "sine-in" => FadeCurve::SineIn,
        "sine_out" | "sineout" | "sine-out" => FadeCurve::SineOut,
        "sine" | "sine_in_out" | "sineinout" | "sin" | "smooth" => FadeCurve::SineInOut,

        // Cubic
        "cubic_in" | "cubicin" | "cubic-in" => FadeCurve::CubicIn,
        "cubic_out" | "cubicout" | "cubic-out" => FadeCurve::CubicOut,
        "cubic_in_out" | "cubicinout" | "cubic-in-out" | "cubic" => FadeCurve::CubicInOut,

        // Exponential (default exponent 2.0)
        "exponential" | "exp" => FadeCurve::Exponential { exponent: 2.0 },

        // Logarithmic
        "log" | "logarithmic" => FadeCurve::Logarithmic,

        // Step
        "step" | "instant" => FadeCurve::Step,

        // Default to linear
        _ => FadeCurve::Linear,
    }
}

/// Convert a CurveSpec into a FadeCurve.
fn parse_curve_spec(spec: &CurveSpec) -> FadeCurve {
    match spec {
        CurveSpec::Name(name) => parse_curve_name(name),
        CurveSpec::Exponential { exp } => FadeCurve::Exponential {
            exponent: *exp as f32,
        },
        CurveSpec::Spline { spline } => FadeCurve::CubicSpline {
            points: spline.iter().map(|[t, v]| (*t as f32, *v as f32)).collect(),
        },
    }
}

/// POST /fades/voice/:name - Start a fade on a voice parameter.
pub async fn fade_voice(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<VoiceFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Find the voice by name
    let voice_id = state
        .with_state(|s| {
            s.voices
                .iter()
                .find(|(_, v)| v.id.raw().to_string() == name || v.config.name == name)
                .map(|(id, _)| *id)
        })
        .await;

    let voice_id = match voice_id {
        Some(id) => id,
        None => {
            // Try parsing as numeric ID
            match name.parse::<u32>() {
                Ok(n) => VoiceId::new(n),
                Err(_) => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::not_found(&format!(
                            "Voice '{}' not found",
                            name
                        ))),
                    ));
                }
            }
        }
    };

    let mut config = FadeConfig::new(
        FadeTarget::Voice(voice_id),
        &req.param,
        req.to,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.from;
    config.curve = parse_curve_spec(&req.curve);

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /fades/group/:path - Start a fade on a group parameter.
pub async fn fade_group(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    Json(req): Json<GroupFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Find the group by path or name
    let group_id = state
        .with_state(|s| {
            s.groups
                .iter()
                .find(|(_, g)| g.id.raw().to_string() == path || format!("{}", g.id) == path)
                .map(|(id, _)| *id)
        })
        .await;

    let group_id = match group_id {
        Some(id) => id,
        None => {
            // Try parsing as numeric ID
            match path.parse::<u32>() {
                Ok(n) => GroupId::new(n),
                Err(_) => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::not_found(&format!(
                            "Group '{}' not found",
                            path
                        ))),
                    ));
                }
            }
        }
    };

    let mut config = FadeConfig::new(
        FadeTarget::Group(group_id),
        &req.param,
        req.to,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.from;
    config.curve = parse_curve_spec(&req.curve);

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /fades/effect/:id - Start a fade on an effect parameter.
pub async fn fade_effect(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<EffectFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let effect_id = id.parse::<u32>().map(EffectId::new).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::bad_request(&format!(
                "Invalid effect ID '{}': must be a number",
                id
            ))),
        )
    })?;

    let mut config = FadeConfig::new(
        FadeTarget::Effect(effect_id),
        &req.param,
        req.to,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.from;
    config.curve = parse_curve_spec(&req.curve);

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Request body for starting a fade on a pattern.
#[derive(Debug, Deserialize)]
pub struct PatternFadeRequest {
    /// Parameter name to fade.
    pub param: String,
    /// Target value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
    /// Optional starting value (defaults to current value).
    #[serde(default)]
    pub from: Option<f32>,
    /// Interpolation curve specification.
    #[serde(default)]
    pub curve: CurveSpec,
}

/// Request body for starting a fade on a melody.
#[derive(Debug, Deserialize)]
pub struct MelodyFadeRequest {
    /// Parameter name to fade.
    pub param: String,
    /// Target value.
    pub to: f32,
    /// Duration in beats.
    pub duration_beats: f64,
    /// Optional starting value (defaults to current value).
    #[serde(default)]
    pub from: Option<f32>,
    /// Interpolation curve specification.
    #[serde(default)]
    pub curve: CurveSpec,
}

/// POST /fades/pattern/:name - Start a fade on a pattern parameter.
pub async fn fade_pattern(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PatternFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let pattern_id = state
        .with_state(|s| {
            s.patterns
                .iter()
                .find(|(_, p)| p.id.raw().to_string() == name || p.content.name == name)
                .map(|(id, _)| *id)
        })
        .await;

    let pattern_id = match pattern_id {
        Some(id) => id,
        None => match name.parse::<u32>() {
            Ok(n) => PatternId::new(n),
            Err(_) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::not_found(&format!(
                        "Pattern '{}' not found",
                        name
                    ))),
                ));
            }
        },
    };

    let mut config = FadeConfig::new(
        FadeTarget::Pattern(pattern_id),
        &req.param,
        req.to,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.from;
    config.curve = parse_curve_spec(&req.curve);

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /fades/melody/:name - Start a fade on a melody parameter.
pub async fn fade_melody(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<MelodyFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let melody_id = state
        .with_state(|s| {
            s.melodies
                .iter()
                .find(|(_, m)| m.id.raw().to_string() == name || m.content.name == name)
                .map(|(id, _)| *id)
        })
        .await;

    let melody_id = match melody_id {
        Some(id) => id,
        None => match name.parse::<u32>() {
            Ok(n) => MelodyId::new(n),
            Err(_) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::not_found(&format!(
                        "Melody '{}' not found",
                        name
                    ))),
                ));
            }
        },
    };

    let mut config = FadeConfig::new(
        FadeTarget::Melody(melody_id),
        &req.param,
        req.to,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.from;
    config.curve = parse_curve_spec(&req.curve);

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Request to cancel a fade.
#[derive(Debug, Deserialize)]
pub struct CancelFadeRequest {
    /// Target type: "group", "voice", or "effect".
    pub target_type: FadeTargetType,
    /// Target name/ID.
    pub target_name: String,
    /// Parameter name.
    pub param: String,
}

/// DELETE /fades - Cancel a fade.
pub async fn cancel_fade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CancelFadeRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let target = match req.target_type {
        FadeTargetType::Group => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(GroupId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid group ID")),
                    )
                })?;
            FadeTarget::Group(id)
        }
        FadeTargetType::Voice => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(VoiceId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid voice ID")),
                    )
                })?;
            FadeTarget::Voice(id)
        }
        FadeTargetType::Effect => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(EffectId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid effect ID")),
                    )
                })?;
            FadeTarget::Effect(id)
        }
    };

    state
        .send(
            FadeMessage::Cancel {
                target,
                param: req.param,
            }
            .into(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /fades - Start a fade (generic endpoint).
pub async fn start_fade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FadeCreate>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let target = match req.target_type {
        FadeTargetType::Group => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(GroupId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid group ID")),
                    )
                })?;
            FadeTarget::Group(id)
        }
        FadeTargetType::Voice => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(VoiceId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid voice ID")),
                    )
                })?;
            FadeTarget::Voice(id)
        }
        FadeTargetType::Effect => {
            let id = req
                .target_name
                .parse::<u32>()
                .map(EffectId::new)
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::bad_request("Invalid effect ID")),
                    )
                })?;
            FadeTarget::Effect(id)
        }
    };

    let mut config = FadeConfig::new(
        target,
        &req.param_name,
        req.target_value,
        Duration::from_beats(req.duration_beats),
    );
    config.from = req.start_value;

    state
        .send(FadeMessage::Start { config }.into())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(&e.to_string())),
            )
        })?;

    Ok(StatusCode::CREATED)
}

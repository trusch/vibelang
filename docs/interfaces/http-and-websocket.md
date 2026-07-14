# HTTP and WebSocket

The CLI starts the API by default on `127.0.0.1:1606` when compiled with the
`api` feature. The server has no authentication, authorization, CSRF protection,
request-size limit, WebSocket origin check, or rate limit. CORS allows every
origin, method, and header. Do not bind an untrusted interface without an
external security boundary.

REST source: [`build_router`](../../crates/vibelang-http/src/lib.rs#L132-L300).
WebSocket source: [`websocket.rs`](../../crates/vibelang-http/src/websocket.rs).
The mechanically extracted method/path registrations and public Serde DTO
inventory live in the [generated route/schema index](../reference/generated/http-routes.md).

## REST endpoint map

| Resource | Exact methods and paths |
|---|---|
| Transport | `GET /transport`; `PATCH /transport`; `POST /transport/start`; `POST /transport/stop`; `POST /transport/seek` |
| Groups | `GET /groups`; `GET/PATCH /groups/{id}`; `POST /groups/{id}/mute`; `/unmute`; `/solo`; `/unsolo`; `PUT /groups/{id}/params/{param}` |
| Voices | `GET/POST /voices`; `GET/PATCH/DELETE /voices/{id}`; `POST /voices/{id}/trigger`; `/stop`; `/note-on`; `/note-off`; `/mute`; `/unmute`; `PUT /voices/{id}/params/{param}` |
| Patterns | `GET/POST /patterns`; `GET/PATCH/DELETE /patterns/{id}`; `POST /patterns/{id}/start`; `/stop`; `PUT /patterns/{id}/params/{param}` |
| Melodies | `GET/POST /melodies`; `GET/PATCH/DELETE /melodies/{id}`; `POST /melodies/{id}/start`; `/stop` |
| Sequences | `GET/POST /sequences`; `GET/PATCH/DELETE /sequences/{id}`; `POST /sequences/{id}/start`; `/stop`; `/pause`; `/resume` |
| Effects | `GET /effects`; `GET/PATCH/DELETE /effects/{id}`; `PUT /effects/{id}/params/{param}` |
| Samples | `GET/POST /samples`; `GET/DELETE /samples/{id}` |
| Synthdefs | `GET /synthdefs`; `GET /synthdefs/{name}` |
| Evaluation | `POST /eval` |
| Live | `GET /live`; `/live/transport`; `/live/fades`; `/live/meters`; `GET /fades` aliases live fades |
| Fades | `POST/DELETE /fades`; `POST /fades/voice/{name}`; `/group/{path}`; `/effect/{id}`; `/pattern/{name}`; `/melody/{name}` |
| Recordings, native | `GET /recordings`; `GET /recordings/{id}`; `POST /recordings/{id}/stop`; `/cancel` |
| WebSocket | `GET /ws` upgrade |
| MIDI, feature-gated | endpoints in the table below |

There is no group-create route although a `GroupCreate` DTO exists, and no
effect-create route although `EffectCreate` exists.

### Feature-gated MIDI endpoints

| Area | Exact methods and paths |
|---|---|
| Devices | `GET /midi/devices`; `POST /midi/input/open`; `/midi/output/open`; `/midi/close` |
| Output | `POST /midi/note/on`; `/midi/note/off`; `/midi/cc` |
| Recording | `POST /midi/record/start`; `/midi/record/stop` |
| Clock/transport | `POST /midi/clock/enable`; `/midi/clock/disable`; `/midi/transport/start`; `/stop`; `/continue` |
| Routes | `GET/DELETE /midi/routes`; `POST /midi/route/keyboard`; `DELETE /midi/route/{index}` |

## REST schemas

Fields suffixed `?` are optional. Maps named `params` contain String→number
values unless noted. Canonical DTO source:
[`models.rs`](../../crates/vibelang-http/src/models.rs); fade/MIDI/eval DTOs also
live beside their route handlers.

### Shared and transport

- `SourceLocation = {file?, line?, column?}`.
- Error is normally `{error, message}`; MIDI uses `{error}`.
- Transport response:
  `{bpm,time_signature:{numerator,denominator},running,current_beat,
  quantization_beats,loop_beats?,loop_beat?,server_time_ms?}`.
- Transport PATCH:
  `{bpm?,time_signature?,quantization_beats?}`; seek `{beat}`.

BPM validates 20..999 and seek requires nonnegative. Time signature is not
validated. PATCH `quantization_beats` is accepted but ignored. Start/stop/PATCH
queue a message and immediately snapshot state, so the response may still be
old.

### Groups, voices, and parameters

- Group response:
  `{name,path,parent_path?,children,node_id,audio_bus,link_synth_node_id?,
  muted,soloed,params,synth_node_ids?,source_location?}`.
- Group PATCH: `{params={} }` and only params are applied.
- `ParamSet = {value, fade_beats?}` for group/voice/pattern/effect; every current
  handler ignores `fade_beats`. Use `/fades`.
- Voice response:
  `{name,synth_name,polyphony,gain,group_path,group_name?,output_bus?,muted,
  soloed,params,sfz_instrument?,vst_instrument?,active_notes?,sustained_notes?,
  running,running_node_id?,source_location?}`.
- Voice create:
  `{name?,synth_name?|synthdef?,polyphony?,gain?,group_path?|group_id?,params={},
  sample?,sfz?}`. Name defaults to generated numeric ID, synth to `default`,
  group to numeric ID 0. Only polyphony/params and identity fields are applied;
  accepted `gain`, `sample`, and `sfz` are ignored.
- Voice update: `{synth_name?,polyphony?,gain?,params={}}`; only params apply.
- Trigger: null or `{params={}}`; note-on `{note:u8,velocity=0.8}`; note-off
  `{note:u8}`. Velocity is otherwise unvalidated.

### Loops: Pattern, Melody, Sequence

- Loop status has `state` equal to `"stopped"`, `"queued"`, `"playing"`, or
  `"queued_stop"`, plus optional `start_beat` and `stop_beat`.
- Pattern response:
  `{name,voice_name,group_path,loop_beats,events:[{beat,params?}],params?,status,
  is_looping,source_location?,step_pattern?}`.
- Pattern create:
  `{name,voice_name,loop_beats=4,events=[],pattern_string?,params={},swing=0}`.
  `pattern_string` is ignored. Voice may be a name or unchecked numeric ID.
- Pattern update `{events?,pattern_string?,loop_beats?,params={}}`; only params
  apply. Start/stop `{quantize_beats?}` accepts but ignores quantization.
- Melody response:
  `{name,voice_name,group_path,loop_beats,events:[{beat,note,frequency?,duration?,
  velocity?,params?}],params?,status,is_looping,source_location?,notes_patterns?}`.
- Melody create:
  `{name,voice_name,loop_beats=4,events=[],melody_string?,params={}}`. It uses
  event beat/note, velocity default 0.8, duration default 1. It ignores
  `melody_string`, top-level params, event frequency, and event params; invalid
  note silently becomes MIDI 60.
- Melody update `{events?,melody_string?,lanes?,loop_beats?,params={}}` ignores
  every field. Start/stop ignores accepted quantization.
- Sequence response:
  `{name,loop_beats,clips:[{type,name,start_beat,end_beat?,duration_beats?,once?}],
  play_once?,active?,source_location?}`.
- Sequence create `{name,loop_beats=16,clips=[]}`. Only pattern/melody/sequence
  clip types are converted; fade/unknown are dropped. Clip name must be numeric,
  missing end/duration becomes four beats, and `once` is ignored.
- Sequence update `{loop_beats?,clips?}` ignores both fields. Start
  `{play_once=false}` is effective.

### Effects, samples, fades, live, and recording

- Effect response:
  `{id,synthdef_name,group_path,node_id?,bus_in?,bus_out?,params,position?,
  vst_plugin?,source_location?}`; PATCH `{params={}}` applies params.
- Sample response:
  `{id,path,buffer_id,num_channels,num_frames,sample_rate,synthdef_name,
  slices?:[{name,start_frame,end_frame}]}`; load `{path,id?}`. Explicit ID must
  be numeric; omitted uses max+1. Duplicate is 409. Path is queued without
  prevalidation and the handler waits 50 ms for state.
- Synthdef response:
  `{name,params:[{name,default_value,min_value?,max_value?}],
  source?:"builtin"|"user"|"stdlib"}`.
- Fade live response:
  `{id,name?,target_type,target_name,param_name,start_value,target_value,
  current_value?,duration_beats,start_beat?,progress}`.
- Generic fade create:
  `{target_type:"group"|"voice"|"effect",target_name,param_name,start_value?,
  target_value,duration_beats}`; cancel
  `{target_type,target_name,param}`. Generic target names must be numeric.
- Specialized fade request:
  `{param,to,duration_beats,from?,curve="linear"}`. Curve is a String alias,
  `{exp:number}`, or `{spline:[[time,value],...]}`; unknown String is linear.
  Specialized endpoints additionally support Pattern/Melody although generic
  `FadeTargetType` does not.
- Live response:
  `{transport,active_synths:[{node_id,synthdef_name,voice_name?,group_path?,
  created_at_beat?}],active_sequences:[{name,start_beat,current_position,
  loop_beats,iteration?,play_once?}],active_fades,active_notes?,patterns_status?,
  melodies_status?}`.
- Meters response maps group path to
  `{peak_left,peak_right,rms_left,rms_right}`.
- Recording response:
  `{id,status:"pending"|"counting_in"|"recording"|"completed"|"cancelled",
  duration_secs?,path?}`. Stop/cancel checks existence and returns 204.

Fade durations/params/target existence in several numeric paths, exponent, and
spline points are not fully validated.

### Eval and MIDI bodies

`POST /eval` accepts `{code}` and returns `{success,result?,error?}`. Missing
integration is 503, job/oneshot failure 500, evaluation/apply failure 400, and
success 200. Extensions remain disabled unless the CLI uses
`--api-allow-extensions`.

MIDI device response is `{id,name,has_input,has_output}`. Device/clock/transport
bodies normally contain `{device_id:u32}`. Note body is
`{device_id,channel:u8,note:u8,velocity?:u8}` (on default 100; off ignores
velocity); CC is `{device_id,channel,cc,value}`; record start
`{device_id,channel?}`, stop `{device_id,quantize?}`. Quantize is ignored and no
take is returned. Keyboard route
`{device_id,voice_id,channel?,note_min?,note_max?,transpose?}` returns
`{message}`. Route list always reports count 0 because the handler cannot query
runtime state. Apart from u8 deserialization, MIDI semantic ranges are not
validated.

## Status and resolution rules

Path IDs generally accept decimal internal IDs or public names resolved by the
handler. Malformed numeric IDs return 400, missing entities 404, and runtime
send failures 500. Creates commonly return 201, deletes 204, and command actions
200. Mutations enqueue runtime messages and frequently return before
reconciliation; successful status means accepted, not confirmed audio state.

## WebSocket `/ws`

Every server frame is:

```json
{"type":"playback.bar","timestamp":1730000000000,"data":{}}
```

Timestamp is Unix milliseconds. On connect the subscription is `*`; the server
sends `hello` with `protocol_version:1` and capabilities, then an initial
`playback.bar` snapshot.

Client text messages contain an `action` of `"subscribe"` or `"unsubscribe"`
and an `events` array of patterns. Empty
subscribe resets to `*`; subscribe adds patterns, so subscribing while `*`
remains does not narrow. Patterns are exact, `*`, or one trailing-star prefix
such as `transport.*`. Invalid JSON/actions and binary frames are ignored.

### Events

| Event | When / data |
|---|---|
| `hello` | Connection metadata/protocol version |
| `playback.tick` | Running sixteenth boundary |
| `playback.bar` | Bar boundary and changed idle snapshot |
| `transport.beat` | Running sixteenth; `{beat,bar,beat_in_bar}` |
| `transport.started`; `transport.stopped` | `{beat}` |
| `transport.bpm` | `{bpm}` |

Polling is 20 Hz and broadcast capacity 1024. A lagged receiver terminates the
sender task/event flow rather than sending a resynchronization snapshot.

### Playback snapshot

`data` contains `transport` and arrays `groups`, `voices`, `patterns`,
`melodies`, `sequences`, `fades`.

- Transport:
  `{playing,beat,bar,beat_in_bar,bpm,time_sig:[numerator,denominator]}`.
- Group:
  `{name,parent,muted,soloed,meter_peak,voices,patterns,melodies,params}`.
- Voice: `{name,synth,muted,soloed,group,active_nodes}`.
- Pattern/Melody: `{name,playing,loop_position,loop_length}`.
- Sequence:
  `{name,playing,paused,position,length,looping,active_clips}`; active clips
  recursively include `type,name,clip_index,progress,nested_clips?`.
- Fade:
  `{target_type,target_name,param,start_value,current_value,target_value,progress}`.

Snapshot bar is zero-based floor. Beats-per-bar uses only the time-signature
numerator. WebSocket shapes are not REST DTO shapes.

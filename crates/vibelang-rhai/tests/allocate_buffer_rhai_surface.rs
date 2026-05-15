//! End-to-end Rhai surface tests for `allocate_buffer(name, frames, channels)`.
//!
//! Drives the API through [`ScriptEngine::execute`]:
//!   1. The script-side handle exposes `.bufnum` as a Rhai FLOAT, suitable
//!      for piping into `voice.set_param("bufnum", h.bufnum)`.
//!   2. The same name in two reloads resolves to the same `BufferId` —
//!      this is the load-bearing property for hot-reload Array persistence.
//!   3. A buffer entry survives in `ScriptState.buffers` with the requested
//!      `(frames, channels)`.
//!   4. Removing the call from the script (next reload) leaves the buffer
//!      out of `ScriptState.buffers` so the runtime diff sees it as
//!      `deleted` and frees the SC buffer.

use vibelang_core::types::BufferId;
use vibelang_rhai::ScriptEngine;

#[test]
fn allocate_buffer_registers_in_script_state() {
    let mut engine = ScriptEngine::new();
    let state = engine
        .execute(
            r#"
            let arr = allocate_buffer("spec_arrays", 65536, 1);
            // Sanity: bufnum is reachable as a property on the handle.
            let _bn = arr.bufnum;
            "#,
        )
        .expect("script must succeed");

    assert_eq!(
        state.buffers.len(),
        1,
        "exactly one buffer should be registered"
    );
    let (id, cfg) = state.buffers.iter().next().unwrap();
    assert_eq!(cfg.name, "spec_arrays");
    assert_eq!(cfg.frames, 65536);
    assert_eq!(cfg.channels, 1);
    // Bufnum sits in the reserved script range (2048..4096).
    assert!(
        (2048..4096).contains(&id.raw()),
        "bufnum {} not in script range",
        id.raw()
    );
}

#[test]
fn allocate_buffer_bufnum_is_set_param_compatible_float() {
    // The whole point of the .bufnum getter is feeding it into set_param,
    // which takes f64. Rhai is strict about INT vs FLOAT overloads, so
    // this would fail to even *parse* the second statement if .bufnum
    // were registered as INT.
    let mut engine = ScriptEngine::new();
    let _state = engine
        .execute(
            r#"
            let arr = allocate_buffer("via_set_param", 1024, 1);
            let v = voice("vox_with_buf").synth("noop_synth");
            v.set_param("bufnum", arr.bufnum);
            "#,
        )
        .expect("script must succeed — set_param must accept arr.bufnum directly");
}

#[test]
fn allocate_buffer_same_name_stable_across_reloads() {
    // The hot-reload property: editing & re-running the script must hand
    // back the SAME bufnum so the runtime's diff treats the buffer entry
    // as unchanged and skips the free + re-alloc cycle.
    let script = r#"
        let arr = allocate_buffer("persistent", 8192, 2);
        let _bn = arr.bufnum;
    "#;

    let mut engine = ScriptEngine::new();
    let state_a = engine.execute(script).expect("first run must succeed");
    let state_b = engine.execute(script).expect("second run must succeed");

    let id_a: BufferId = *state_a.buffers.keys().next().unwrap();
    let id_b: BufferId = *state_b.buffers.keys().next().unwrap();
    assert_eq!(
        id_a, id_b,
        "same allocate_buffer(name=...) must yield the same BufferId across reloads"
    );

    // Configs match too — diff would see this as `unchanged`.
    assert_eq!(state_a.buffers[&id_a], state_b.buffers[&id_b]);
}

#[test]
fn allocate_buffer_dropped_call_removes_buffer_entry() {
    // Reload-shaped scenario: first the script has `allocate_buffer("x", ...)`,
    // then the user removes the call. The new ScriptState.buffers must NOT
    // carry the old entry — that's how the runtime diff knows to free the
    // backend buffer.
    let mut engine = ScriptEngine::new();
    let state_with = engine
        .execute(r#"let _arr = allocate_buffer("ephemeral", 1024, 1);"#)
        .expect("with-call must succeed");
    assert_eq!(state_with.buffers.len(), 1);

    let state_without = engine.execute("// no allocate_buffer call").unwrap();
    assert!(
        state_without.buffers.is_empty(),
        "removing the allocate_buffer call must drop the buffer from the new ScriptState"
    );
}

#[test]
fn allocate_buffer_distinct_names_distinct_bufnums() {
    let mut engine = ScriptEngine::new();
    let state = engine
        .execute(
            r#"
            let a = allocate_buffer("first",  1024, 1);
            let b = allocate_buffer("second", 1024, 1);
            let c = allocate_buffer("third",  2048, 2);
            "#,
        )
        .expect("script must succeed");

    assert_eq!(state.buffers.len(), 3);
    let ids: std::collections::HashSet<_> = state.buffers.keys().copied().collect();
    assert_eq!(
        ids.len(),
        3,
        "three distinct names → three distinct BufferIds"
    );
}

//! A SynthDef the server never built must be observable, and re-sendable.
//!
//! scsynth accepts a `/d_recv` whose graph it cannot construct. It prints
//! `exception in GraphDef_Recv: UGen 'X' not installed.` on its own stdout
//! — which this process never sees — and replies `/done` regardless. So
//! "the server acked the def" proves nothing. The refusal surfaces only
//! later, as `/fail /s_new SynthDef not found`, and that reply carries
//! neither a node id nor a def name.
//!
//! Board-measured on super-gamma (2026-08-17): `cassette` was logged as
//! "accepted by scsynth" at 13.983 and refused at `/s_new` 0.9 s later, the
//! reload reported success, and one performance side went silent. Because
//! the deploy path had already registered the def's content hash, every
//! later reload skipped re-sending it — one refusal silenced that voice for
//! the life of the process, and recalling the same patch could not repair
//! it.
//!
//! These tests stand up a fake scsynth that tells the same lie, and pin
//! both halves: the refusal is attributed to the right def, and the def's
//! content hash is dropped so the next evaluation re-sends it.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use vibelang_core::{AddAction, Backend, NodeId, ParamMap, ScsynthBackend};

/// The synthdef hash registry these tests assert on is process-global, and
/// `ScsynthBackend::connect` clears it — that is deliberate, a fresh server
/// holds no defs. So two of these tests running at once would have one
/// test's connect wipe the other's registration. Hold this for the whole of
/// each test: connect, register, and assert have to be one unit.
fn registry_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    // Poisoning only means some other test already failed; it says nothing
    // about the registry, so recover rather than cascading the failure.
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// How a fake scsynth answers `/s_new`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SNewReply {
    /// The def is there: the node goes live.
    Go,
    /// The def is not there, exactly as scsynth phrases it.
    NotFound,
}

struct FakeScsynth {
    addr: String,
    running: Arc<AtomicBool>,
}

impl FakeScsynth {
    /// Bind a fake server that answers the handshake, acks every `/d_recv`
    /// (the lie), and answers `/s_new` as told.
    fn spawn(reply: SNewReply) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind fake scsynth");
        let addr = socket.local_addr().expect("local addr").to_string();
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("read timeout");
        let running = Arc::new(AtomicBool::new(true));

        let thread_running = running.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while thread_running.load(Ordering::Relaxed) {
                let (size, peer) = match socket.recv_from(&mut buf) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) else {
                    continue;
                };
                let OscPacket::Message(msg) = packet else {
                    continue;
                };
                let replies: Vec<OscMessage> = match msg.addr.as_str() {
                    "/status" => vec![OscMessage {
                        addr: "/status.reply".into(),
                        args: vec![
                            OscType::Int(1),
                            OscType::Int(0),
                            OscType::Int(0),
                            OscType::Int(0),
                            OscType::Int(0),
                            OscType::Float(0.0),
                            OscType::Float(0.0),
                            OscType::Double(44100.0),
                            OscType::Double(44100.0),
                        ],
                    }],
                    // The lie: a def the server cannot build is still acked.
                    "/d_recv" | "/d_load" => vec![OscMessage {
                        addr: "/done".into(),
                        args: vec![OscType::String(msg.addr.clone())],
                    }],
                    "/s_new" => {
                        let node = match msg.args.get(1) {
                            Some(OscType::Int(n)) => *n,
                            _ => 0,
                        };
                        match reply {
                            SNewReply::Go => vec![OscMessage {
                                addr: "/n_go".into(),
                                args: vec![
                                    OscType::Int(node),
                                    OscType::Int(0),
                                    OscType::Int(-1),
                                    OscType::Int(-1),
                                    OscType::Int(0),
                                ],
                            }],
                            SNewReply::NotFound => vec![OscMessage {
                                addr: "/fail".into(),
                                args: vec![
                                    OscType::String("/s_new".into()),
                                    OscType::String("SynthDef not found".into()),
                                ],
                            }],
                        }
                    }
                    "/sync" => {
                        let id = match msg.args.first() {
                            Some(OscType::Int(n)) => *n,
                            _ => 0,
                        };
                        vec![OscMessage {
                            addr: "/synced".into(),
                            args: vec![OscType::Int(id)],
                        }]
                    }
                    _ => Vec::new(),
                };
                for reply in replies {
                    if let Ok(bytes) = encoder::encode(&OscPacket::Message(reply)) {
                        let _ = socket.send_to(&bytes, peer);
                    }
                }
            }
        });

        Self { addr, running }
    }
}

impl Drop for FakeScsynth {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Poll until the server's answer has been processed, or give up. The
/// reply arrives on the backend's listener thread, so there is nothing to
/// await; a bounded poll is the whole synchronisation.
fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(value) = probe() {
            return Some(value);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

#[tokio::test]
async fn refused_s_new_names_the_synthdef_and_frees_it_for_resend() {
    let _serialized = registry_guard();
    let server = FakeScsynth::spawn(SNewReply::NotFound);
    let backend = ScsynthBackend::connect(&server.addr)
        .await
        .expect("connect to fake scsynth");

    // The deploy path registers a hash after the server acks the /d_recv,
    // which is how a byte-identical body is skipped on the next reload.
    // For a def the server refused, that skip is what makes the silence
    // permanent — so the refusal has to undo it.
    vibelang_dsp::register_synthdef_hash("ghost_voice".into(), 0xDEAD_BEEF);
    assert_eq!(
        vibelang_dsp::get_synthdef_hash("ghost_voice"),
        Some(0xDEAD_BEEF),
        "precondition: the deploy path considers this def sent"
    );

    backend
        .create_synth(
            "ghost_voice",
            NodeId::new(4001),
            NodeId::new(0),
            AddAction::Tail,
            &ParamMap::new(),
        )
        .await
        .expect("s_new goes out");

    let missing = wait_for(|| {
        let taken = backend.take_missing_synthdefs();
        (!taken.is_empty()).then_some(taken)
    })
    .expect("the refused /s_new is reported");

    assert_eq!(missing.len(), 1, "one refusal, one report: {missing:?}");
    assert_eq!(
        missing[0].def, "ghost_voice",
        "the report names the def the /fail did not"
    );
    assert_eq!(missing[0].node, NodeId::new(4001), "and the node it lost");

    assert_eq!(
        vibelang_dsp::get_synthdef_hash("ghost_voice"),
        None,
        "a refused def must be re-sent by the next evaluation, not skipped \
         as unchanged — otherwise one refusal silences that voice for the \
         life of the process"
    );

    assert!(
        backend.take_missing_synthdefs().is_empty(),
        "taking the report clears it, so the next reload is judged on its own"
    );
}

/// Re-sending the def is only half the repair: the voice that was refused
/// has no node on the server, and the reload differ decides what to rebuild
/// by comparing content hashes. The re-sent body hashes to exactly what it
/// hashed before, so without the refusal generation the differ reads
/// "unchanged", leaves the voice missing, and the instrument stays half
/// silent — which is what a recall gesture measured on the board.
#[tokio::test]
async fn a_refused_def_reads_as_changed_once_so_its_voices_are_rebuilt() {
    let _serialized = registry_guard();
    let server = FakeScsynth::spawn(SNewReply::NotFound);
    let backend = ScsynthBackend::connect(&server.addr)
        .await
        .expect("connect to fake scsynth");

    // The def as it stood when the reload that lost it was applied.
    const BODY: u64 = 0x0BAD_0F00;
    vibelang_dsp::register_synthdef_hash("stranded_voice".into(), BODY);
    let snapshotted = vibelang_dsp::get_synthdef_deploy_hash("stranded_voice")
        .expect("a deployed def has a deploy hash");

    backend
        .create_synth(
            "stranded_voice",
            NodeId::new(4003),
            NodeId::new(0),
            AddAction::Tail,
            &ParamMap::new(),
        )
        .await
        .expect("s_new goes out");

    wait_for(|| {
        let taken = backend.take_missing_synthdefs();
        (!taken.is_empty()).then_some(taken)
    })
    .expect("the refused /s_new is reported");

    // The next evaluation re-sends the def — same source, same bytes, so
    // `deploy_synthdef_ir` registers the identical content hash.
    vibelang_dsp::register_synthdef_hash("stranded_voice".into(), BODY);
    assert_eq!(
        vibelang_dsp::get_synthdef_hash("stranded_voice"),
        Some(BODY),
        "the re-sent body is byte-identical, as it is in the real reload"
    );

    let after_resend = vibelang_dsp::get_synthdef_deploy_hash("stranded_voice")
        .expect("still deployed after the re-send");
    assert_ne!(
        after_resend, snapshotted,
        "a def that went missing and came back must read as changed, or the \
         differ leaves the voice that was refused with no node and the \
         instrument stays half silent"
    );

    // ...and exactly once. A generation that kept moving would recreate
    // every dependent voice on every later reload.
    vibelang_dsp::register_synthdef_hash("stranded_voice".into(), BODY);
    assert_eq!(
        vibelang_dsp::get_synthdef_deploy_hash("stranded_voice"),
        Some(after_resend),
        "one refusal rebuilds the voice once, it does not cause churn forever"
    );
}

#[tokio::test]
async fn a_synth_that_goes_live_is_not_reported_missing() {
    let _serialized = registry_guard();
    let server = FakeScsynth::spawn(SNewReply::Go);
    let backend = ScsynthBackend::connect(&server.addr)
        .await
        .expect("connect to fake scsynth");

    vibelang_dsp::register_synthdef_hash("real_voice".into(), 0x1234_5678);
    backend
        .create_synth(
            "real_voice",
            NodeId::new(4002),
            NodeId::new(0),
            AddAction::Tail,
            &ParamMap::new(),
        )
        .await
        .expect("s_new goes out");

    // Give the /n_go the same window the refusal test gives the /fail, so
    // this cannot pass merely by being read too early.
    assert!(
        wait_for(
            || (vibelang_dsp::get_synthdef_hash("real_voice") != Some(0x1234_5678)).then_some(())
        )
        .is_none(),
        "a synth that went live must not have its def hash dropped"
    );
    assert!(
        backend.take_missing_synthdefs().is_empty(),
        "a graph the server built is whole"
    );
}

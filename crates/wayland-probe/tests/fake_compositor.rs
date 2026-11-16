//! Integration test: drive the real client wire path against a minimal fake
//! compositor speaking the Wayland wire protocol over a `socketpair`.
//!
//! The fake server answers `wl_display.get_registry` (by remembering the
//! registry object id), replays a fixed list of `wl_registry.global` events
//! and completes every `wl_display.sync` with `wl_callback.done`. This covers
//! `connection::collect_globals` end-to-end without a real compositor and
//! without mutating process environment variables.
//!
//! Wire format (Wayland 1.x, little-endian): each message is `object-id:u32` +
//! `size:u16 << 16 | opcode:u16`, followed by args; strings are
//! `len:u32` (including NUL) + bytes padded to 4.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;

use wayland_client::Connection;
use wayland_probe::classify::build_report;
use wayland_probe::connection::{collect_globals, ConnectFailureKind};
use wayland_probe::model::{ConnectionInfo, ConnectionStatus, OverallStatus, ProtocolStatus};

/// Globals a wlroots-like compositor would advertise (subset).
const ADVERTISED: &[(u32, &str, u32)] = &[
    (1, "wl_compositor", 6),
    (2, "wl_shm", 1),
    (3, "wl_seat", 8),
    (4, "xdg_wm_base", 5),
    (5, "zwlr_layer_shell_v1", 4),
    (6, "xdg_activation_v1", 1),
    (7, "wp_fractional_scale_manager_v1", 1),
    (8, "wp_viewporter", 1),
    (9, "zwlr_foreign_toplevel_manager_v1", 3),
    (10, "zwp_text_input_manager_v3", 1),
    (11, "wl_output", 4),
];

fn push_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Encode one `wl_registry.global` event (opcode 0).
fn encode_global(registry_id: u32, name: u32, interface: &str, version: u32) -> Vec<u8> {
    let mut args = Vec::new();
    push_u32(&mut args, name);
    let iface_bytes = format!("{interface}\0").into_bytes();
    push_u32(&mut args, iface_bytes.len() as u32);
    args.extend_from_slice(&iface_bytes);
    while args.len() % 4 != 0 {
        args.push(0);
    }
    push_u32(&mut args, version);
    let mut msg = Vec::with_capacity(8 + args.len());
    push_u32(&mut msg, registry_id);
    push_u32(&mut msg, ((8 + args.len()) as u32) << 16); // opcode 0
    msg.extend_from_slice(&args);
    msg
}

/// Encode one `wl_callback.done` event (opcode 0).
fn encode_done(callback_id: u32, serial: u32) -> Vec<u8> {
    let mut msg = Vec::with_capacity(12);
    push_u32(&mut msg, callback_id);
    push_u32(&mut msg, 12u32 << 16); // opcode 0
    push_u32(&mut msg, serial);
    msg
}

/// Serve one client until it disconnects. `wl_display` is object 1;
/// `sync` is request opcode 0, `get_registry` is request opcode 1, and both
/// carry a single trailing `new_id` argument.
fn serve(mut stream: UnixStream) {
    let mut registry_id: Option<u32> = None;
    let mut header = [0u8; 8];
    loop {
        if stream.read_exact(&mut header).is_err() {
            return; // client went away; test is done
        }
        let object = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let size_op = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let size = (size_op >> 16) as usize;
        let opcode = (size_op & 0xffff) as u16;
        let mut body = vec![0u8; size.saturating_sub(8)];
        if size > 8 && stream.read_exact(&mut body).is_err() {
            return;
        }
        if object != 1 {
            continue; // the probe sends nothing else
        }
        let new_id = if body.len() >= 4 {
            Some(u32::from_le_bytes(
                body[body.len() - 4..].try_into().unwrap(),
            ))
        } else {
            None
        };
        match (opcode, new_id) {
            (1, Some(id)) => registry_id = Some(id), // get_registry
            (0, Some(callback)) => {
                // sync: flush the global list, then complete the roundtrip.
                let registry = match registry_id {
                    Some(id) => id,
                    None => return, // sync before get_registry: protocol error
                };
                let mut batch = Vec::new();
                for (name, interface, version) in ADVERTISED {
                    batch.extend(encode_global(registry, *name, interface, *version));
                }
                batch.extend(encode_done(callback, 0));
                if stream.write_all(&batch).is_err() {
                    return;
                }
            }
            _ => {}
        }
    }
}

fn connected_info() -> ConnectionInfo {
    ConnectionInfo {
        status: ConnectionStatus::Connected,
        wayland_display: Some("fake-0".to_string()),
        xdg_runtime_dir: None,
        error: None,
    }
}

#[test]
fn wire_roundtrip_against_fake_compositor() {
    let (client_end, server_end) = UnixStream::pair().expect("socketpair");
    let server = thread::spawn(move || serve(server_end));
    // The server must not outlive a wedged test: it exits when we drop the
    // connection below, and a client-side failure surfaces as an Err here.
    let conn = Connection::from_socket(client_end).expect("from_socket");
    let globals = match collect_globals(&conn) {
        Ok(globals) => globals,
        Err(failure) => panic!(
            "collect_globals failed: {:?} {}",
            failure.kind, failure.message
        ),
    };
    drop(conn);
    server.join().expect("server thread panicked");

    // Raw globals: every advertised entry arrived, with registry ids intact.
    assert_eq!(globals.len(), ADVERTISED.len());
    for (name, interface, version) in ADVERTISED {
        let hit = globals
            .iter()
            .find(|g| g.id == *name && g.interface == *interface && g.version == *version);
        assert!(hit.is_some(), "global {interface} v{version} not collected");
    }

    // Full pipeline: wire globals -> pure classification -> report verdict.
    let report = build_report(connected_info(), &globals);
    assert_eq!(report.compositor.family, "wlroots");
    assert_eq!(report.compositor.wl_compositor_version, Some(6));
    // every required AND recommended protocol was advertised -> pass
    assert_eq!(report.overall, OverallStatus::Pass);
    assert!(report
        .required
        .iter()
        .all(|e| e.status == ProtocolStatus::Ok));
    let missing: Vec<&str> = report
        .missing_recommended
        .iter()
        .map(String::as_str)
        .collect();
    assert!(
        missing.is_empty(),
        "unexpected missing recommended: {missing:?}"
    );
    // json output stays parseable
    let json = wayland_probe::model::to_compact_json(&report);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["connection"]["status"], "connected");
    assert_eq!(value["overall"], "pass");
}

#[test]
fn collect_globals_maps_backend_failure_to_registry_error() {
    // A server that closes the socket immediately: the client roundtrip hits
    // EOF; the probe must return a RegistryError instead of panicking.
    let (client_end, server_end) = UnixStream::pair().expect("socketpair");
    drop(server_end);
    let conn = Connection::from_socket(client_end).expect("from_socket");
    match collect_globals(&conn) {
        Ok(globals) => panic!("expected failure, got {globals:?}"),
        Err(failure) => assert_eq!(failure.kind, ConnectFailureKind::RegistryError),
    }
}

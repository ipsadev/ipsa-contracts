mod common;

use common::{mk_payload, recv_packet_for, router_roots, setup, setup_packet_flow, PacketFlow};
use soroban_sdk::{vec, Bytes, BytesN};
use stellar_ibc_router::Packet;

fn zero() -> [u8; 32] {
    [0u8; 32]
}

fn send(f: &PacketFlow, value: &[u8]) -> Packet {
    let payload = mk_payload(&f.env, &f.port_id, value);
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);

    Packet {
        sequence,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    }
}

#[test]
fn the_root_event_has_the_shape_the_light_client_expects() {
    let f = setup_packet_flow();

    send(&f, b"hello");

    let roots = router_roots(&f.env, &f.router_id);

    assert_eq!(roots.len(), 1, "exactly one root event per provable write");
    assert_ne!(
        roots[0],
        zero(),
        "a populated tree must not report the empty root"
    );
}

#[test]
fn every_provable_write_republishes_the_root() {
    let f = setup_packet_flow();
    let packet = send(&f, b"hello");
    let after_commitment = *router_roots(&f.env, &f.router_id).last().unwrap();

    f.router.recv_packet(
        &recv_packet_for(&f, 9, b"hello"),
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );
    let after_receipt = *router_roots(&f.env, &f.router_id).last().unwrap();

    assert_ne!(
        after_commitment, after_receipt,
        "a receipt and its ack are provable state and must move the root"
    );

    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );
    let after_acknowledgement = *router_roots(&f.env, &f.router_id).last().unwrap();

    assert_ne!(
        after_receipt, after_acknowledgement,
        "clearing a commitment must move the root"
    );
}

#[test]
fn clearing_a_commitment_moves_the_root_back() {
    let f = setup_packet_flow();
    let empty = f.router.ibc_root();
    let packet = send(&f, b"hello");
    let populated = f.router.ibc_root();

    assert_ne!(empty, populated);

    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );

    assert_eq!(
        f.router.ibc_root(),
        empty,
        "removing the only leaf returns to the empty root"
    );
}

#[test]
fn commit_root_republishes_without_changing_state() {
    let f = setup_packet_flow();

    send(&f, b"hello");

    let before = f.router.ibc_root();
    let republished = f.router.commit_root();

    assert_eq!(
        before, republished,
        "an idle republish must not move the root"
    );

    let roots = router_roots(&f.env, &f.router_id);

    assert_eq!(roots.len(), 1, "the republish emits exactly one root event");
    assert_eq!(
        roots[0],
        before.to_array(),
        "and it carries the unchanged root"
    );
}

#[test]
fn an_untouched_router_reports_the_empty_root() {
    let (env, _router_id, router, _lc) = setup();

    assert_eq!(router.ibc_root(), BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn the_root_tracks_the_stored_value() {
    let first = setup_packet_flow();
    let second = setup_packet_flow();

    send(&first, b"one value");
    send(&second, b"another value");

    assert_ne!(
        first.router.ibc_root(),
        second.router.ibc_root(),
        "the same path holding a different commitment must give a different root"
    );
}

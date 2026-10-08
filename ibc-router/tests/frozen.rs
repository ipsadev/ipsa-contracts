mod common;

use common::{mk_payload, recv_packet_for, setup_packet_flow, PacketFlow};
use soroban_sdk::{vec, Bytes, String};
use stellar_ibc_router::{DataKey, Error, Packet};
use stellar_mock_light_client::MockLightClient;

fn freeze(f: &PacketFlow) {
    f.env.as_contract(&f.router_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::Frozen(f.client_id.clone()), &true);
    });
    assert!(f.router.frozen(&f.client_id));
}

fn sent(f: &PacketFlow) -> Packet {
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
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

fn proof(f: &PacketFlow) -> Bytes {
    Bytes::from_slice(&f.env, b"proof")
}

#[test]
fn a_frozen_client_refuses_to_send() {
    let f = setup_packet_flow();
    freeze(&f);

    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let result = f
        .router
        .try_send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    assert_eq!(result, Err(Ok(Error::ClientFrozen.into())));
}

#[test]
fn a_frozen_client_refuses_to_receive() {
    let f = setup_packet_flow();
    freeze(&f);

    let packet = recv_packet_for(&f, 1, b"hello");
    let result = f.router.try_recv_packet(&packet, &proof(&f), &10);

    assert_eq!(result, Err(Ok(Error::ClientFrozen.into())));
    assert!(!f.router.has_packet_receipt(&f.client_id, &1));
}

#[test]
fn a_frozen_client_refuses_an_acknowledgement() {
    let f = setup_packet_flow();
    let packet = sent(&f);
    freeze(&f);

    let acks = vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")];
    let result = f
        .router
        .try_acknowledge_packet(&packet, &acks, &proof(&f), &10);

    assert_eq!(result, Err(Ok(Error::ClientFrozen.into())));
    assert!(f
        .router
        .packet_commitment(&f.client_id, &packet.sequence)
        .is_some());
}

#[test]
fn a_frozen_client_refuses_a_timeout() {
    let f = setup_packet_flow();
    let packet = sent(&f);
    freeze(&f);

    let result = f.router.try_timeout_packet(&packet, &proof(&f), &10);

    assert_eq!(result, Err(Ok(Error::ClientFrozen.into())));
    assert!(f
        .router
        .packet_commitment(&f.client_id, &packet.sequence)
        .is_some());
}

#[test]
fn migrating_a_frozen_client_lets_packets_through_again() {
    let f = setup_packet_flow();
    freeze(&f);

    let replacement = f.env.register(MockLightClient, ());
    f.router.migrate_client(
        &f.client_id,
        &replacement,
        &Bytes::from_slice(&f.env, b"client-state"),
        &Bytes::from_slice(&f.env, b"consensus-state"),
        &40,
    );

    let packet = recv_packet_for(&f, 1, b"hello");
    f.router.recv_packet(&packet, &proof(&f), &10);

    assert!(f.router.has_packet_receipt(&f.client_id, &1));
}

#[test]
fn another_client_is_unaffected_by_a_freeze() {
    let f = setup_packet_flow();
    freeze(&f);

    let other = f.router.create_client(
        &String::from_str(&f.env, "mock"),
        &Bytes::from_slice(&f.env, b"cs"),
        &Bytes::from_slice(&f.env, b"cons"),
        &1,
    );
    f.router.register_counterparty(
        &other,
        &String::from_str(&f.env, "07-tendermint-9"),
        &vec![&f.env, Bytes::from_slice(&f.env, b"ibc")],
    );

    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    f.router.send_packet(&other, &2_000, &vec![&f.env, payload]);
}

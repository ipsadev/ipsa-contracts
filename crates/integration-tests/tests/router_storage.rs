use integration_tests::router::{
    flow_committing_in, make_payload, recv_packet_in, sent_packet_in, setup_packet_flow, PacketFlow,
};
use integration_tests::tendermint::{Entry, HEIGHT};
use soroban_sdk::{vec, Bytes, BytesN, IntoVal, Symbol, Val};
use stellar_ibc_router::Packet;

fn send(f: &PacketFlow, value: &[u8]) -> (u64, Packet) {
    let payload = make_payload(&f.env, &f.port_id, value);
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);

    let packet = Packet {
        sequence,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };

    (sequence, packet)
}

fn app_acks(f: &PacketFlow) -> soroban_sdk::Vec<Bytes> {
    vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")]
}

fn acknowledge(f: &PacketFlow, packet: &Packet, proof: &Bytes) {
    f.router
        .acknowledge_packet(packet, &app_acks(f), proof, &HEIGHT);
}

#[test]
fn a_sent_packet_is_committed_and_an_acknowledgement_clears_it() {
    let env = soroban_sdk::Env::default();
    let expected = sent_packet_in(&env, 1, 2_000, b"hello");
    let acks = vec![&env, Bytes::from_slice(&env, b"ack-bytes")];
    let f = flow_committing_in(env, &[Entry::Acknowledgement(expected, acks)], 1);

    assert!(f.router.packet_commitment(&f.client_id, &1).is_none());

    let (sequence, packet) = send(&f, b"hello");

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_some());

    acknowledge(&f, &packet, &f.proofs[0].clone());

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_none());
}

#[test]
fn a_received_packet_leaves_a_receipt_and_an_acknowledgement() {
    let env = soroban_sdk::Env::default();
    let packet = recv_packet_in(&env, 3, b"hello");
    let f = flow_committing_in(env, &[Entry::Packet(packet.clone())], 1);

    assert!(!f.router.has_packet_receipt(&f.client_id, &3));
    assert!(f.router.acknowledgement(&f.client_id, &3).is_none());

    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert!(f.router.has_packet_receipt(&f.client_id, &3));
    assert!(!f.router.has_packet_receipt(&f.client_id, &4));
    assert!(f.router.acknowledgement(&f.client_id, &3).is_some());
}

#[test]
fn provable_paths_are_keyed_by_distinct_discriminators() {
    let env = soroban_sdk::Env::default();
    let inbound = recv_packet_in(&env, 1, b"hello");
    let outbound = sent_packet_in(&env, 1, 2_000, b"hello");
    let acks = vec![&env, Bytes::from_slice(&env, b"ack-bytes")];
    let f = flow_committing_in(
        env,
        &[
            Entry::Packet(inbound.clone()),
            Entry::Acknowledgement(outbound, acks),
        ],
        1,
    );
    let (sequence, packet) = send(&f, b"hello");

    f.router
        .recv_packet(&inbound, &f.proofs[0].clone(), &HEIGHT);

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_some());
    assert!(f.router.has_packet_receipt(&f.client_id, &sequence));
    assert!(f.router.acknowledgement(&f.client_id, &sequence).is_some());

    acknowledge(&f, &packet, &f.proofs[1].clone());

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_none());
    assert!(f.router.has_packet_receipt(&f.client_id, &sequence));
    assert!(f.router.acknowledgement(&f.client_id, &sequence).is_some());
}

#[test]
fn provable_state_cannot_be_written_from_outside_the_router() {
    let f = setup_packet_flow();
    let client: Val = f.client_id.into_val(&f.env);
    let sequence: Val = 7u64.into_val(&f.env);
    let hash: Val = BytesN::from_array(&f.env, &[0xAB; 32]).into_val(&f.env);

    for (function, args) in [
        (
            "set_packet_commitment",
            vec![&f.env, client, sequence, hash],
        ),
        ("delete_packet_commitment", vec![&f.env, client, sequence]),
        ("set_packet_receipt", vec![&f.env, client, sequence]),
        ("set_ack_commitment", vec![&f.env, client, sequence, hash]),
    ] {
        let result = f.env.try_invoke_contract::<Val, soroban_sdk::Error>(
            &f.router_id,
            &Symbol::new(&f.env, function),
            args,
        );

        assert!(result.is_err(), "{function} must not be callable");
    }

    assert!(f.router.packet_commitment(&f.client_id, &7).is_none());
    assert!(!f.router.has_packet_receipt(&f.client_id, &7));
    assert!(f.router.acknowledgement(&f.client_id, &7).is_none());
}

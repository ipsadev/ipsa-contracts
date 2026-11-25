use integration_tests::router::{
    flow_committing_in, make_payload, COUNTERPARTY_CLIENT_ID, LOCAL_CLIENT_ID,
};
use integration_tests::tendermint::{Entry, HEIGHT};
use soroban_sdk::{contract, contractimpl, vec, Bytes, Env, String};
use stellar_ibc_router::{
    ics24::{commit_v2_acknowledgement, error_ack_hash},
    Error, OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback, Packet,
};

#[contract]
pub struct FailingApp;

#[contractimpl]
impl FailingApp {
    pub fn on_recv_packet(_env: Env, _cb: OnRecvPacketCallback) -> Bytes {
        panic!("the application rejects the packet")
    }

    pub fn on_acknowledgement_packet(_env: Env, _cb: OnAcknowledgementPacketCallback) {}

    pub fn on_timeout_packet(_env: Env, _cb: OnTimeoutPacketCallback) {}
}

#[test]
fn a_failing_application_gets_the_universal_error_acknowledgement() {
    let env = Env::default();
    let port = String::from_str(&env, "failing");
    let packet = Packet {
        sequence: 1,
        source_client: String::from_str(&env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(&env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![&env, make_payload(&env, &port, b"hello")],
    };
    let f = flow_committing_in(env, &[Entry::Packet(packet.clone())], 1);
    let app = f.env.register(FailingApp, ());
    f.router.register_port(&port, &app);

    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    let error_ack: Bytes = error_ack_hash(&f.env).into();
    let expected = commit_v2_acknowledgement(&f.env, &vec![&f.env, error_ack]);

    assert!(f.router.has_packet_receipt(&f.client_id, &1));
    assert_eq!(f.router.acknowledgement(&f.client_id, &1), Some(expected));
}

#[test]
fn a_succeeding_application_keeps_its_own_acknowledgement() {
    let env = Env::default();
    let port = String::from_str(&env, "transfer");
    let packet = Packet {
        sequence: 1,
        source_client: String::from_str(&env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(&env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![&env, make_payload(&env, &port, b"hello")],
    };
    let f = flow_committing_in(env, &[Entry::Packet(packet.clone())], 1);

    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    let expected =
        commit_v2_acknowledgement(&f.env, &vec![&f.env, Bytes::from_slice(&f.env, b"hello")]);

    assert_eq!(f.router.acknowledgement(&f.client_id, &1), Some(expected));
}

#[test]
fn a_packet_with_two_payloads_is_not_received() {
    let env = Env::default();
    let port = String::from_str(&env, "transfer");
    let packet = Packet {
        sequence: 1,
        source_client: String::from_str(&env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(&env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            &env,
            make_payload(&env, &port, b"one"),
            make_payload(&env, &port, b"two"),
        ],
    };
    let f = flow_committing_in(env, &[Entry::Packet(packet.clone())], 1);

    let result = f
        .router
        .try_recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(result, Err(Ok(Error::MultiplePayloadsNotSupported.into())));
    assert!(!f.router.has_packet_receipt(&f.client_id, &1));
}

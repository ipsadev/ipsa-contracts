mod common;

use common::{mk_payload, setup_packet_flow};
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
    let f = setup_packet_flow();
    let port = String::from_str(&f.env, "failing");
    let app = f.env.register(FailingApp, ());
    f.router.register_port(&port, &app);

    let packet = Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, mk_payload(&f.env, &port, b"hello")],
    };

    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    let error_ack: Bytes = error_ack_hash(&f.env).into();
    let expected = commit_v2_acknowledgement(&f.env, &vec![&f.env, error_ack]);

    assert!(f.router.has_packet_receipt(&f.client_id, &1));
    assert_eq!(f.router.acknowledgement(&f.client_id, &1), Some(expected));
}

#[test]
fn a_succeeding_application_keeps_its_own_acknowledgement() {
    let f = setup_packet_flow();
    let packet = Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, mk_payload(&f.env, &f.port_id, b"hello")],
    };

    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    let expected =
        commit_v2_acknowledgement(&f.env, &vec![&f.env, Bytes::from_slice(&f.env, b"hello")]);

    assert_eq!(f.router.acknowledgement(&f.client_id, &1), Some(expected));
}

#[test]
fn a_packet_with_two_payloads_is_not_received() {
    let f = setup_packet_flow();
    let packet = Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            mk_payload(&f.env, &f.port_id, b"one"),
            mk_payload(&f.env, &f.port_id, b"two"),
        ],
    };

    let result = f
        .router
        .try_recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    assert_eq!(result, Err(Ok(Error::MultiplePayloadsNotSupported.into())));
    assert!(!f.router.has_packet_receipt(&f.client_id, &1));
}

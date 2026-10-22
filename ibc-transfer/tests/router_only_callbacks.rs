mod common;

use common::{outbound_timeout, outbound_value, setup, ENCODING};
use soroban_sdk::{testutils::Address as _, Address, Bytes, String};
use stellar_ibc_transfer::{OnAcknowledgementPacketCallback, OnRecvPacketCallback, Payload};

fn payload(f: &common::Fixture, sender: &Address) -> Payload {
    Payload {
        source_port: String::from_str(&f.env, "transfer"),
        dest_port: String::from_str(&f.env, "transfer"),
        version: String::from_str(&f.env, "ics20-1"),
        encoding: String::from_str(&f.env, ENCODING),
        value: outbound_value(&f.env, sender, 400),
    }
}

#[test]
fn only_the_router_can_deliver_a_packet() {
    let f = setup();
    let receiver = Address::generate(&f.env);
    let callback = OnRecvPacketCallback {
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        sequence: 1,
        payload: payload(&f, &receiver),
    };

    f.env.set_auths(&[]);

    assert!(f.transfer.try_on_recv_packet(&callback).is_err());
}

#[test]
fn only_the_router_can_deliver_an_acknowledgement() {
    let f = setup();
    let sender = Address::generate(&f.env);
    let callback = OnAcknowledgementPacketCallback {
        source_client: f.source_client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        sequence: 1,
        payload: payload(&f, &sender),
        acknowledgement: Bytes::from_slice(&f.env, b"\xff\xff"),
    };

    f.env.set_auths(&[]);

    assert!(f.transfer.try_on_acknowledgement_packet(&callback).is_err());
}

#[test]
fn only_the_router_can_deliver_a_timeout() {
    let f = setup();
    let sender = Address::generate(&f.env);
    let callback = outbound_timeout(&f, 1, &sender, 400);

    f.env.set_auths(&[]);

    assert!(f.transfer.try_on_timeout_packet(&callback).is_err());
}

mod common;

use common::{mk_payload, sent_packet, setup_elapsed_packet_flow};
use soroban_sdk::{vec, Bytes};
use stellar_ibc_router::Error;

#[test]
fn timeout_packet_retires_the_commitment_once_the_counterparty_passes_the_timeout() {
    let f = setup_elapsed_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    f.router.timeout_packet(
        &sent_packet(&f, sequence, 2_000, b"hello"),
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_none());
    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

#[test]
fn a_timed_out_packet_cannot_be_timed_out_again() {
    let f = setup_elapsed_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    let packet = sent_packet(&f, sequence, 2_000, b"hello");
    let proof = Bytes::from_slice(&f.env, b"proof");

    f.router.timeout_packet(&packet, &proof, &10);

    let result = f.router.try_timeout_packet(&packet, &proof, &10);

    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn a_timed_out_packet_cannot_be_acknowledged() {
    let f = setup_elapsed_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    let packet = sent_packet(&f, sequence, 2_000, b"hello");
    let proof = Bytes::from_slice(&f.env, b"proof");

    f.router.timeout_packet(&packet, &proof, &10);

    let acknowledgements = vec![&f.env, Bytes::from_slice(&f.env, b"ack")];
    let result = f
        .router
        .try_acknowledge_packet(&packet, &acknowledgements, &proof, &10);

    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn an_acknowledged_packet_cannot_be_timed_out_or_acknowledged_again() {
    let f = setup_elapsed_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    let packet = sent_packet(&f, sequence, 2_000, b"hello");
    let proof = Bytes::from_slice(&f.env, b"proof");
    let acknowledgements = vec![&f.env, Bytes::from_slice(&f.env, b"ack")];

    f.router
        .acknowledge_packet(&packet, &acknowledgements, &proof, &10);

    let timeout = f.router.try_timeout_packet(&packet, &proof, &10);
    let acknowledgement = f
        .router
        .try_acknowledge_packet(&packet, &acknowledgements, &proof, &10);

    assert_eq!(timeout, Err(Ok(Error::NoCommitmentForSequence.into())));
    assert_eq!(
        acknowledgement,
        Err(Ok(Error::NoCommitmentForSequence.into()))
    );
    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

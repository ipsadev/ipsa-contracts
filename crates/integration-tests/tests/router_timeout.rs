use integration_tests::router::{acknowledgement_then_timeout_flow, make_payload, timeout_flow};
use integration_tests::tendermint::HEIGHT;
use soroban_sdk::{vec, Bytes};
use stellar_ibc_router::Error;

#[test]
fn timeout_packet_retires_the_commitment_once_the_counterparty_passes_the_timeout() {
    let (f, packet) = timeout_flow(1, b"hello");
    let payload = make_payload(&f.env, &f.port_id, b"hello");
    let sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    f.router
        .timeout_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert!(f
        .router
        .packet_commitment(&f.client_id, &sequence)
        .is_none());
    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

#[test]
fn a_timed_out_packet_cannot_be_timed_out_again() {
    let (f, packet) = timeout_flow(1, b"hello");
    let payload = make_payload(&f.env, &f.port_id, b"hello");
    let _sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    let proof = f.proofs[0].clone();

    f.router.timeout_packet(&packet, &proof, &HEIGHT);

    let result = f.router.try_timeout_packet(&packet, &proof, &HEIGHT);

    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn a_timed_out_packet_cannot_be_acknowledged() {
    let (f, packet) = timeout_flow(1, b"hello");
    let payload = make_payload(&f.env, &f.port_id, b"hello");
    let _sequence = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    let proof = f.proofs[0].clone();

    f.router.timeout_packet(&packet, &proof, &HEIGHT);

    let acknowledgements = vec![&f.env, Bytes::from_slice(&f.env, b"ack")];
    let result = f
        .router
        .try_acknowledge_packet(&packet, &acknowledgements, &proof, &HEIGHT);

    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn an_acknowledged_packet_cannot_be_timed_out_or_acknowledged_again() {
    let (f, packet, acknowledgement_proof, absence_proof) =
        acknowledgement_then_timeout_flow(1, b"hello", b"ack");
    let payload = make_payload(&f.env, &f.port_id, b"hello");

    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    let acknowledgements = vec![&f.env, Bytes::from_slice(&f.env, b"ack")];

    f.router
        .acknowledge_packet(&packet, &acknowledgements, &acknowledgement_proof, &HEIGHT);

    let timeout = f
        .router
        .try_timeout_packet(&packet, &absence_proof, &HEIGHT);
    let acknowledgement = f.router.try_acknowledge_packet(
        &packet,
        &acknowledgements,
        &acknowledgement_proof,
        &HEIGHT,
    );

    assert_eq!(timeout, Err(Ok(Error::NoCommitmentForSequence.into())));
    assert_eq!(
        acknowledgement,
        Err(Ok(Error::NoCommitmentForSequence.into()))
    );
    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

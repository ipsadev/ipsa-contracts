mod common;

use common::{mk_payload, setup_packet_flow};
use soroban_sdk::vec;
use stellar_ibc_router::Error;

#[test]
fn send_packet_rejects_more_than_one_payload() {
    let f = setup_packet_flow();
    let first = mk_payload(&f.env, &f.port_id, b"first");
    let second = mk_payload(&f.env, &f.port_id, b"second");

    let result = f
        .router
        .try_send_packet(&f.client_id, &2_000, &vec![&f.env, first, second]);

    assert_eq!(result, Err(Ok(Error::MultiplePayloadsNotSupported.into())));
    assert!(f.router.packet_commitment(&f.client_id, &1).is_none());
    assert_eq!(f.router.port_packets_in_flight(&f.port_id), 0);
}

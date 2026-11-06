mod common;

use common::*;
use sha3::{Digest, Keccak256};
use soroban_sdk::{Bytes, BytesN};
use stellar_ibc_fast_fill::Error;

#[test]
fn a_refund_notice_tells_the_sending_contract_who_filled_the_order() {
    let f = setup();
    let order = order(&f, 1);
    let key = f.fast_fill.fill(&f.provider, &order, &provider_evm(&f));

    let gmp_sequence = f.fast_fill.notify_refund(&key);

    let sent = f.gmp.sent().expect("a notice was sent");
    let selector = Keccak256::digest(b"assignRefund(bytes32,address)");

    let mut call = selector[..4].to_vec();

    call.extend_from_slice(&key.to_array());
    call.extend_from_slice(&[0u8; 12]);
    call.extend_from_slice(&[0x22u8; 20]);

    assert_eq!(gmp_sequence, 7);
    assert_eq!(sent.sender, f.fast_fill_addr);
    assert_eq!(sent.source_client_id, f.stellar_client);
    assert_eq!(sent.receiver, order.sender);
    assert_eq!(sent.payload, Bytes::from_slice(&f.env, &call));
    assert_eq!(sent.timeout_timestamp, NOW + 3_600);
}

#[test]
fn a_refund_notice_is_sent_once() {
    let f = setup();
    let key = f
        .fast_fill
        .fill(&f.provider, &order(&f, 1), &provider_evm(&f));

    f.fast_fill.notify_refund(&key);

    let result = f.fast_fill.try_notify_refund(&key);

    assert!(matches!(result, Err(Ok(error)) if error == Error::AlreadyNotified.into()));
}

#[test]
fn a_delivered_order_has_no_refund_to_notify() {
    let f = setup();
    let order = order(&f, 1);
    let key = f.fast_fill.fill(&f.provider, &order, &provider_evm(&f));

    deliver(&f, &order);

    let result = f.fast_fill.try_notify_refund(&key);

    assert!(matches!(result, Err(Ok(error)) if error == Error::AlreadySettled.into()));
}

#[test]
fn an_unfilled_order_has_no_refund_to_notify() {
    let f = setup();

    let result = f
        .fast_fill
        .try_notify_refund(&BytesN::from_array(&f.env, &[9u8; 32]));

    assert!(matches!(result, Err(Ok(error)) if error == Error::NotFilled.into()));
}

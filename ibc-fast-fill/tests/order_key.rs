mod common;

use common::*;
use sha2::{Digest, Sha256};

#[test]
fn the_order_key_matches_the_packed_encoding_the_sepolia_contract_hashes() {
    let f = setup();
    let order = order(&f, 42);

    let memo = order.memo.to_string();
    let source_client = order.source_client.to_string();

    let mut preimage = 42u64.to_be_bytes().to_vec();

    preimage.extend_from_slice(&[0u8; 16]);
    preimage.extend_from_slice(&(AMOUNT as u128).to_be_bytes());
    preimage.extend_from_slice(&Sha256::digest(source_client.as_bytes()));
    preimage.extend_from_slice(&Sha256::digest(WETH.as_bytes()));
    preimage.extend_from_slice(&Sha256::digest(memo.as_bytes()));

    let expected: [u8; 32] = Sha256::digest(&preimage).into();

    assert_eq!(f.fast_fill.order_key(&order).to_array(), expected);
}

#[test]
fn the_order_key_matches_the_vector_the_sepolia_tests_assert() {
    let f = setup();
    let order = stellar_ibc_fast_fill::Order {
        source_client: soroban_sdk::String::from_str(&f.env, "sp1-stellar-0"),
        dest_client: f.stellar_client.clone(),
        sequence: 42,
        sender: soroban_sdk::String::from_str(&f.env, FAST_TRANSFER),
        denom: soroban_sdk::String::from_str(&f.env, WETH),
        amount: 1_000_000,
        memo: soroban_sdk::String::from_str(
            &f.env,
            "{\"dest_callback\":{\"address\":\"CFASTFILL\"},\"fast_fill\":\"GRECIPIENT:2500:1600\"}",
        ),
    };

    assert_eq!(
        f.fast_fill.order_key(&order).to_array(),
        [
            0xa3, 0xed, 0x83, 0x91, 0xa5, 0xa8, 0xe5, 0x90, 0x68, 0x81, 0x83, 0x09, 0x87, 0x92,
            0xb1, 0x10, 0x50, 0xc8, 0x45, 0x65, 0x60, 0xad, 0x9a, 0x0b, 0x1f, 0x11, 0x0d, 0xf2,
            0x36, 0x3e, 0x0d, 0xff,
        ]
    );
}

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

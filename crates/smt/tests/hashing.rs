use ipsa_smt::{empty, inner_hash, key_index, leaf_hash};
use soroban_sdk::{Bytes, BytesN, Env};

#[test]
fn leaf_hash_is_sha256_of_the_tagged_pair() {
    let env = Env::default();
    let key = Bytes::from_slice(&env, b"path");
    let value = Bytes::from_slice(&env, b"value");

    let mut expected = Bytes::from_slice(&env, &[0x00]);
    expected.append(&env.crypto().sha256(&key).to_bytes().into());
    expected.append(&env.crypto().sha256(&value).to_bytes().into());

    assert_eq!(
        leaf_hash(&env, &key, &value),
        env.crypto().sha256(&expected).to_bytes()
    );
}

#[test]
fn inner_hash_is_sha256_of_the_tagged_children() {
    let env = Env::default();
    let left = BytesN::from_array(&env, &[0x11; 32]);
    let right = BytesN::from_array(&env, &[0x22; 32]);

    let mut expected = Bytes::from_slice(&env, &[0x01]);
    expected.append(&Bytes::from_array(&env, &left.to_array()));
    expected.append(&Bytes::from_array(&env, &right.to_array()));

    assert_eq!(
        inner_hash(&env, &left, &right),
        env.crypto().sha256(&expected).to_bytes()
    );
}

#[test]
fn the_leaf_and_inner_tags_separate_the_domains() {
    let env = Env::default();
    let left = BytesN::from_array(&env, &[0x11; 32]);
    let right = BytesN::from_array(&env, &[0x22; 32]);

    let mut tagged_as_leaf = Bytes::from_slice(&env, &[0x00]);
    tagged_as_leaf.append(&Bytes::from_array(&env, &left.to_array()));
    tagged_as_leaf.append(&Bytes::from_array(&env, &right.to_array()));

    assert_ne!(
        inner_hash(&env, &left, &right),
        env.crypto().sha256(&tagged_as_leaf).to_bytes(),
        "an inner node must not collide with a leaf over the same bytes"
    );
}

#[test]
fn empty_subtrees_collapse_to_zero() {
    let env = Env::default();
    let zero = empty(&env);

    assert_eq!(inner_hash(&env, &zero, &zero), zero);
    assert_ne!(
        inner_hash(&env, &BytesN::from_array(&env, &[1u8; 32]), &zero),
        BytesN::from_array(&env, &[1u8; 32]),
        "a lone child must still be folded, not passed through"
    );
}

#[test]
fn the_child_order_changes_the_inner_hash() {
    let env = Env::default();
    let left = BytesN::from_array(&env, &[0x11; 32]);
    let right = BytesN::from_array(&env, &[0x22; 32]);

    assert_ne!(
        inner_hash(&env, &left, &right),
        inner_hash(&env, &right, &left)
    );
}

#[test]
fn the_key_index_is_the_first_eight_digest_bytes_big_endian() {
    let env = Env::default();
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let digest = env.crypto().sha256(&key).to_bytes().to_array();

    let mut leading = [0u8; 8];
    leading.copy_from_slice(&digest[..8]);

    assert_eq!(key_index(&env, &key), u64::from_be_bytes(leading));
}

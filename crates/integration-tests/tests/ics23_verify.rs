use ics23::NonExistenceProof;
use integration_tests::ics23::{
    absence_proof, b, exist_cp, existence, iavl_inner, iavl_leaf_prefix, keys, membership_fixture,
    merkle_bytes, multistore, non_membership_fixture, nonexist_cp, receipt_key, receipts,
    reference_root, store_keys, Receipts, STORE,
};
use ipsa_ics23::{verify_membership, verify_non_membership};
use soroban_sdk::Env;

fn reference_accepts_absence(
    r: &Receipts,
    key: &[u8],
    left: Option<usize>,
    right: Option<usize>,
) -> bool {
    let ne = nonexist_cp(NonExistenceProof {
        key: key.to_vec(),
        left: left.map(|i| r.tree.proofs[i].clone()),
        right: right.map(|i| r.tree.proofs[i].clone()),
    });

    ics23::verify_non_membership::<ics23::HostFunctionsManager>(
        &ne,
        &ics23::iavl_spec(),
        &r.tree.root.to_vec(),
        key,
    ) && ics23::verify_membership::<ics23::HostFunctionsManager>(
        &exist_cp(r.multistore.proof.clone()),
        &ics23::tendermint_spec(),
        &r.multistore.root.to_vec(),
        STORE,
        &r.tree.root,
    )
}

fn crate_accepts_absence(
    r: &Receipts,
    key: &[u8],
    left: Option<usize>,
    right: Option<usize>,
) -> bool {
    let env = Env::default();

    verify_non_membership(
        &env,
        &r.multistore.root,
        &b(&env, &absence_proof(r, key, left, right)),
        &store_keys(&env, key),
    )
}

fn assert_absence(key: &[u8], left: Option<usize>, right: Option<usize>, expected: bool) {
    let r = receipts();

    assert_eq!(
        reference_accepts_absence(&r, key, left, right),
        expected,
        "reference ics23"
    );
    assert_eq!(
        crate_accepts_absence(&r, key, left, right),
        expected,
        "ipsa-ics23"
    );
}

#[test]
fn membership_accepts_valid_two_layer_proof() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_tampered_value() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, b"not-the-committed-value")
    ));
}

#[test]
fn membership_rejects_wrong_path() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &store_keys(&env, b"10-stellar-0\x03\x00\x00\x00\x00\x00\x00\x00\x08"),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_wrong_root() {
    let env = Env::default();
    let f = membership_fixture();
    let mut bad_root = f.root;
    bad_root[0] ^= 0xff;

    assert!(!verify_membership(
        &env,
        &bad_root,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_a_store_key_split_across_levels() {
    let env = Env::default();
    let f = membership_fixture();

    let split_tail = [b"c".as_slice(), &f.key].concat();
    let split = keys(&env, &[b"ib", &split_tail]);

    assert_eq!(
        [b"ib".as_slice(), &split_tail].concat(),
        [STORE, &f.key].concat()
    );
    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &split,
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_the_whole_path_as_one_key() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &keys(&env, &[&[STORE, &f.key].concat()]),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_keys_in_the_wrong_order() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &keys(&env, &[&f.key, STORE]),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_an_extra_key() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &keys(&env, &[b"", STORE, &f.key]),
        &b(&env, &f.value)
    ));
}

#[test]
fn non_membership_accepts_valid_absence_proof() {
    let env = Env::default();
    let f = non_membership_fixture();

    assert!(verify_non_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key)
    ));
}

#[test]
fn non_membership_rejects_wrong_root() {
    let env = Env::default();
    let f = non_membership_fixture();
    let mut bad = f.root;
    bad[3] ^= 0xff;

    assert!(!verify_non_membership(
        &env,
        &bad,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key)
    ));
}

#[test]
fn non_membership_rejects_existence_proof() {
    let env = Env::default();
    let f = membership_fixture();

    assert!(!verify_non_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key)
    ));
}

#[test]
fn non_membership_rejects_a_store_key_split_across_levels() {
    let env = Env::default();
    let f = non_membership_fixture();

    let split_tail = [b"c".as_slice(), &f.key].concat();

    assert!(!verify_non_membership(
        &env,
        &f.root,
        &b(&env, &f.proof),
        &keys(&env, &[b"ib", &split_tail])
    ));
}

#[test]
fn non_membership_accepts_adjacent_neighbours_under_one_parent() {
    assert_absence(&receipt_key(2), Some(0), Some(1), true);
}

#[test]
fn non_membership_accepts_adjacent_neighbours_across_the_root() {
    assert_absence(&receipt_key(4), Some(1), Some(2), true);
}

#[test]
fn non_membership_accepts_rightmost_left_neighbour_alone() {
    assert_absence(&receipt_key(8), Some(3), None, true);
}

#[test]
fn non_membership_accepts_leftmost_right_neighbour_alone() {
    assert_absence(&receipt_key(0), None, Some(0), true);
}

#[test]
fn non_membership_rejects_present_key_with_lone_left_neighbour() {
    assert_absence(&receipt_key(5), Some(1), None, false);
}

#[test]
fn non_membership_rejects_present_key_with_lone_right_neighbour() {
    assert_absence(&receipt_key(5), None, Some(3), false);
}

#[test]
fn non_membership_rejects_present_key_with_non_adjacent_neighbours() {
    assert_absence(&receipt_key(3), Some(0), Some(2), false);
}

#[test]
fn non_membership_rejects_lone_left_neighbour_that_is_not_rightmost() {
    assert_absence(&receipt_key(2), Some(0), None, false);
}

#[test]
fn non_membership_rejects_lone_right_neighbour_that_is_not_leftmost() {
    assert_absence(&receipt_key(6), None, Some(3), false);
}

#[test]
fn non_membership_rejects_swapped_neighbours() {
    assert_absence(&receipt_key(4), Some(2), Some(1), false);
}

#[test]
fn non_membership_rejects_a_third_proof_level() {
    let env = Env::default();
    let r = receipts();
    let absent = receipt_key(4);
    let ne = NonExistenceProof {
        key: absent.clone(),
        left: Some(r.tree.proofs[1].clone()),
        right: Some(r.tree.proofs[2].clone()),
    };
    let outer = multistore(r.multistore.root);
    let proof = merkle_bytes(&[
        nonexist_cp(ne),
        exist_cp(r.multistore.proof.clone()),
        exist_cp(outer.proof),
    ]);

    assert!(!verify_non_membership(
        &env,
        &outer.root,
        &b(&env, &proof),
        &keys(&env, &[STORE, STORE, &absent])
    ));
}

#[test]
fn membership_rejects_leaf_without_the_iavl_prefix() {
    let env = Env::default();
    let key = receipt_key(1);
    let inners = vec![iavl_inner(1, 2, [0x11u8; 32], true)];
    let leaf = existence(&key, &[0x01], &[0x00], &inners);
    let subroot = reference_root(&key, &[0x01], &[0x00], &inners);
    let outer = multistore(subroot);
    let proof = merkle_bytes(&[exist_cp(leaf), exist_cp(outer.proof)]);

    assert!(!verify_membership(
        &env,
        &outer.root,
        &b(&env, &proof),
        &store_keys(&env, &key),
        &b(&env, &[0x01])
    ));
}

#[test]
fn membership_rejects_inner_node_with_the_leaf_prefix() {
    let env = Env::default();
    let key = receipt_key(1);
    let leaf_prefix = iavl_leaf_prefix();
    let mut inner = iavl_inner(1, 2, [0x11u8; 32], true);
    inner.0[0] = 0x00;
    let inners = vec![inner];
    let leaf = existence(&key, &[0x01], &leaf_prefix, &inners);
    let subroot = reference_root(&key, &[0x01], &leaf_prefix, &inners);
    let outer = multistore(subroot);
    let proof = merkle_bytes(&[exist_cp(leaf), exist_cp(outer.proof)]);

    assert!(!verify_membership(
        &env,
        &outer.root,
        &b(&env, &proof),
        &store_keys(&env, &key),
        &b(&env, &[0x01])
    ));
}

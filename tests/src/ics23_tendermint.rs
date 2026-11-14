use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, LeafOp, LengthOp,
    NonExistenceProof,
};
use prost::Message;
use sha2::{Digest, Sha256};
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Bytes, BytesN, Env, String};
use stellar_tendermint_light_client::{
    Height, TendermintClientState, TendermintConsensusState, TendermintLightClient,
    TendermintLightClientClient, TrustThreshold,
};

const STORE: &[u8] = b"ibc";

fn sha(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);

    h.finalize().into()
}

fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if n == 0 {
            break;
        }
    }

    out
}

fn leaf_op(prefix: &[u8]) -> LeafOp {
    LeafOp {
        hash: HashOp::Sha256 as i32,
        prehash_key: HashOp::NoHash as i32,
        prehash_value: HashOp::Sha256 as i32,
        length: LengthOp::VarProto as i32,
        prefix: prefix.to_vec(),
    }
}

fn reference_root(
    key: &[u8],
    value: &[u8],
    prefix: &[u8],
    inners: &[(Vec<u8>, Vec<u8>)],
) -> [u8; 32] {
    let mut data = Vec::new();
    data.extend_from_slice(prefix);
    data.extend_from_slice(&varint(key.len() as u64));
    data.extend_from_slice(key);
    let value_hash = sha(value);
    data.extend_from_slice(&varint(value_hash.len() as u64));
    data.extend_from_slice(&value_hash);
    let mut acc = sha(&data);

    for (p, s) in inners {
        let mut d = Vec::new();
        d.extend_from_slice(p);
        d.extend_from_slice(&acc);
        d.extend_from_slice(s);
        acc = sha(&d);
    }

    acc
}

fn existence(
    key: &[u8],
    value: &[u8],
    prefix: &[u8],
    inners: &[(Vec<u8>, Vec<u8>)],
) -> ExistenceProof {
    ExistenceProof {
        key: key.to_vec(),
        value: value.to_vec(),
        leaf: Some(leaf_op(prefix)),
        path: inners
            .iter()
            .map(|(p, s)| InnerOp {
                hash: HashOp::Sha256 as i32,
                prefix: p.clone(),
                suffix: s.clone(),
            })
            .collect(),
    }
}

fn exist_cp(e: ExistenceProof) -> CommitmentProof {
    CommitmentProof {
        proof: Some(Proof::Exist(e)),
    }
}

fn nonexist_cp(ne: NonExistenceProof) -> CommitmentProof {
    CommitmentProof {
        proof: Some(Proof::Nonexist(ne)),
    }
}

fn merkle_bytes(cps: &[CommitmentProof]) -> Vec<u8> {
    let mut out = Vec::new();
    for cp in cps {
        let encoded = cp.encode_to_vec();
        out.push(0x0a);
        out.extend_from_slice(&varint(encoded.len() as u64));
        out.extend_from_slice(&encoded);
    }

    out
}

fn setup(env: &Env, root: [u8; 32], height: u64) -> (TendermintLightClientClient<'static>, String) {
    let router = soroban_sdk::Address::generate(env);
    let addr = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(env, &addr);
    let client_id = String::from_str(env, "07-tendermint-0");

    let client_state = TendermintClientState {
        chain_id: String::from_str(env, "test-chain-0"),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: 1_000_000,
        unbonding_period_secs: 2_000_000,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: height,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    };
    let consensus_state = TendermintConsensusState {
        timestamp_secs: 1,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        root: BytesN::from_array(env, &root),
    };

    client.initialise(
        &client_id,
        &client_state.to_xdr(env),
        &consensus_state.to_xdr(env),
        &height,
    );

    (client, client_id)
}

fn b(env: &Env, data: &[u8]) -> Bytes {
    Bytes::from_slice(env, data)
}

const VERSION: i64 = 5;

fn zigzag(n: i64) -> Vec<u8> {
    varint(((n << 1) ^ (n >> 63)) as u64)
}

fn iavl_leaf_prefix() -> Vec<u8> {
    [zigzag(0), zigzag(1), zigzag(VERSION)].concat()
}

fn iavl_inner(height: i64, size: i64, sibling: [u8; 32], we_are_left: bool) -> (Vec<u8>, Vec<u8>) {
    let mut prefix = [zigzag(height), zigzag(size), zigzag(VERSION)].concat();
    prefix.push(0x20);

    if we_are_left {
        (prefix, [vec![0x20], sibling.to_vec()].concat())
    } else {
        prefix.extend_from_slice(&sibling);
        prefix.push(0x20);

        (prefix, Vec::new())
    }
}

fn hash_inner(prefix: &[u8], child: &[u8; 32], suffix: &[u8]) -> [u8; 32] {
    sha(&[prefix, child, suffix].concat())
}

struct IavlTree {
    root: [u8; 32],
    proofs: Vec<ExistenceProof>,
}

fn iavl_tree(leaves: &[(&[u8], &[u8]); 4]) -> IavlTree {
    let leaf_prefix = iavl_leaf_prefix();
    let leaf_hashes: Vec<[u8; 32]> = leaves
        .iter()
        .map(|(key, value)| reference_root(key, value, &leaf_prefix, &[]))
        .collect();

    let pair = |left: [u8; 32], right: [u8; 32], height: i64, size: i64| {
        let (prefix, suffix) = iavl_inner(height, size, right, true);

        hash_inner(&prefix, &left, &suffix)
    };

    let middle = [
        pair(leaf_hashes[0], leaf_hashes[1], 1, 2),
        pair(leaf_hashes[2], leaf_hashes[3], 1, 2),
    ];
    let root = pair(middle[0], middle[1], 2, 4);

    let proofs = (0..4)
        .map(|i| {
            let inners = vec![
                iavl_inner(1, 2, leaf_hashes[i ^ 1], i % 2 == 0),
                iavl_inner(2, 4, middle[(i / 2) ^ 1], i / 2 == 0),
            ];
            let (key, value) = leaves[i];
            let proof = existence(key, value, &leaf_prefix, &inners);
            assert_eq!(reference_root(key, value, &leaf_prefix, &inners), root);

            proof
        })
        .collect();

    IavlTree { root, proofs }
}

struct Multistore {
    root: [u8; 32],
    proof: ExistenceProof,
}

fn multistore(ibc_root: [u8; 32]) -> Multistore {
    let bank = reference_root(b"bank", &[0x42u8; 32], &[0x00], &[]);
    let inners = vec![([vec![0x01u8], bank.to_vec()].concat(), Vec::new())];

    Multistore {
        root: reference_root(STORE, &ibc_root, &[0x00], &inners),
        proof: existence(STORE, &ibc_root, &[0x00], &inners),
    }
}

fn receipt_key(sequence: u8) -> Vec<u8> {
    let mut key = b"10-stellar-0\x02\x00\x00\x00\x00\x00\x00\x00".to_vec();
    key.push(sequence);

    key
}

fn store_path(key: &[u8]) -> Vec<u8> {
    [STORE, key].concat()
}

struct Receipts {
    keys: [Vec<u8>; 4],
    tree: IavlTree,
    multistore: Multistore,
}

fn receipts() -> Receipts {
    let keys = [
        receipt_key(1),
        receipt_key(3),
        receipt_key(5),
        receipt_key(7),
    ];
    let tree = iavl_tree(&[
        (&keys[0], &[0x01]),
        (&keys[1], &[0x01]),
        (&keys[2], &[0x01]),
        (&keys[3], &[0x01]),
    ]);
    let multistore = multistore(tree.root);

    Receipts {
        keys,
        tree,
        multistore,
    }
}

fn absence_proof(r: &Receipts, key: &[u8], left: Option<usize>, right: Option<usize>) -> Vec<u8> {
    let ne = NonExistenceProof {
        key: key.to_vec(),
        left: left.map(|i| r.tree.proofs[i].clone()),
        right: right.map(|i| r.tree.proofs[i].clone()),
    };

    merkle_bytes(&[nonexist_cp(ne), exist_cp(r.multistore.proof.clone())])
}

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

fn contract_accepts_absence(
    r: &Receipts,
    key: &[u8],
    left: Option<usize>,
    right: Option<usize>,
) -> bool {
    let env = Env::default();
    let (client, cid) = setup(&env, r.multistore.root, 10);

    client.verify_non_membership(
        &cid,
        &10,
        &b(&env, &absence_proof(r, key, left, right)),
        &b(&env, &store_path(key)),
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
        contract_accepts_absence(&r, key, left, right),
        expected,
        "contract"
    );
}

struct MembershipFixture {
    proof: Vec<u8>,
    path: Vec<u8>,
    value: Vec<u8>,
    root: [u8; 32],
}

fn membership_fixture() -> MembershipFixture {
    let r = receipts();

    MembershipFixture {
        proof: merkle_bytes(&[
            exist_cp(r.tree.proofs[2].clone()),
            exist_cp(r.multistore.proof.clone()),
        ]),
        path: store_path(&r.keys[2]),
        value: vec![0x01],
        root: r.multistore.root,
    }
}

#[test]
fn membership_accepts_valid_two_layer_proof() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &b(&env, &f.path),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_tampered_value() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &b(&env, &f.path),
        &b(&env, b"not-the-committed-value")
    ));
}

#[test]
fn membership_rejects_wrong_path() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    let mut wrong = STORE.to_vec();
    wrong.extend_from_slice(b"10-stellar-0\x03\x00\x00\x00\x00\x00\x00\x00\x08");

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &b(&env, &wrong),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_wrong_root() {
    let env = Env::default();
    let f = membership_fixture();
    let mut bad_root = f.root;
    bad_root[0] ^= 0xff;
    let (client, cid) = setup(&env, bad_root, 10);

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &b(&env, &f.path),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_unknown_height() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(!client.verify_membership(
        &cid,
        &99,
        &b(&env, &f.proof),
        &b(&env, &f.path),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_when_frozen() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    client.update_state_on_misbehaviour(&cid, &b(&env, b""));

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &b(&env, &f.path),
        &b(&env, &f.value)
    ));
}

struct NonMembershipFixture {
    proof: Vec<u8>,
    path: Vec<u8>,
    root: [u8; 32],
}

fn non_membership_fixture() -> NonMembershipFixture {
    let r = receipts();
    let absent = receipt_key(4);

    NonMembershipFixture {
        proof: absence_proof(&r, &absent, Some(1), Some(2)),
        path: store_path(&absent),
        root: r.multistore.root,
    }
}

#[test]
fn non_membership_accepts_valid_absence_proof() {
    let env = Env::default();
    let f = non_membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(client.verify_non_membership(&cid, &10, &b(&env, &f.proof), &b(&env, &f.path)));
}

#[test]
fn non_membership_rejects_wrong_root() {
    let env = Env::default();
    let f = non_membership_fixture();
    let mut bad = f.root;
    bad[3] ^= 0xff;
    let (client, cid) = setup(&env, bad, 10);

    assert!(!client.verify_non_membership(&cid, &10, &b(&env, &f.proof), &b(&env, &f.path)));
}

#[test]
fn non_membership_rejects_existence_proof() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(!client.verify_non_membership(&cid, &10, &b(&env, &f.proof), &b(&env, &f.path)));
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
    let (client, cid) = setup(&env, outer.root, 10);

    assert!(!client.verify_non_membership(
        &cid,
        &10,
        &b(&env, &proof),
        &b(&env, &[STORE, STORE, &absent].concat())
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
    let (client, cid) = setup(&env, outer.root, 10);

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &proof),
        &b(&env, &store_path(&key)),
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
    let (client, cid) = setup(&env, outer.root, 10);

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &proof),
        &b(&env, &store_path(&key)),
        &b(&env, &[0x01])
    ));
}

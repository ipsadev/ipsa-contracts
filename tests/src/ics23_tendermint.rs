use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, LeafOp, LengthOp,
    NonExistenceProof,
};
use prost::Message;
use sha2::{Digest, Sha256};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Bytes, BytesN, Env, String};
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
    let addr = env.register(TendermintLightClient, ());
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

struct MembershipFixture {
    proof: Vec<u8>,
    path: Vec<u8>,
    value: Vec<u8>,
    root: [u8; 32],
}

fn membership_fixture() -> MembershipFixture {
    let inner_key = b"10-stellar-0\x03\x00\x00\x00\x00\x00\x00\x00\x07";
    let value = vec![0xabu8; 32];
    let inner_inners = vec![
        (vec![0x01u8, 0x02], vec![0xaau8; 32]),
        (vec![0x03u8], vec![0xbbu8; 33]),
    ];
    let inner = existence(inner_key, &value, &[0x00], &inner_inners);
    let subroot = reference_root(inner_key, &value, &[0x00], &inner_inners);

    let outer_inners = vec![(vec![0x01u8], vec![0xccu8; 32])];
    let outer = existence(STORE, &subroot, &[0x00], &outer_inners);
    let root = reference_root(STORE, &subroot, &[0x00], &outer_inners);

    let proof = merkle_bytes(&[exist_cp(inner), exist_cp(outer)]);

    let mut path = STORE.to_vec();
    path.extend_from_slice(inner_key);

    MembershipFixture {
        proof,
        path,
        value,
        root,
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
    let absent = b"10-stellar-0\x02\x00\x00\x00\x00\x00\x00\x00\x05";
    let left_key = b"10-stellar-0\x02\x00\x00\x00\x00\x00\x00\x00\x01";
    let left_inners = vec![(vec![0x01u8], vec![0xa1u8; 32])];
    let left = existence(left_key, b"neighbor", &[0x00], &left_inners);
    let subroot = reference_root(left_key, b"neighbor", &[0x00], &left_inners);

    let ne = NonExistenceProof {
        key: absent.to_vec(),
        left: Some(left),
        right: None,
    };

    let outer_inners = vec![(vec![0x01u8], vec![0xd0u8; 32])];
    let outer = existence(STORE, &subroot, &[0x00], &outer_inners);
    let root = reference_root(STORE, &subroot, &[0x00], &outer_inners);

    let proof = merkle_bytes(&[nonexist_cp(ne), exist_cp(outer)]);

    let mut path = STORE.to_vec();
    path.extend_from_slice(absent);

    NonMembershipFixture { proof, path, root }
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

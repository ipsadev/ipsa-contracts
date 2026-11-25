use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, LeafOp, LengthOp,
    NonExistenceProof,
};
use prost::Message;
use sha2::{Digest, Sha256};
use soroban_sdk::{Bytes, Env, Vec as SorobanVec};

pub const STORE: &[u8] = b"ibc";

pub fn sha(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);

    h.finalize().into()
}

pub fn varint(mut n: u64) -> Vec<u8> {
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

pub fn leaf_op(prefix: &[u8]) -> LeafOp {
    LeafOp {
        hash: HashOp::Sha256 as i32,
        prehash_key: HashOp::NoHash as i32,
        prehash_value: HashOp::Sha256 as i32,
        length: LengthOp::VarProto as i32,
        prefix: prefix.to_vec(),
    }
}

pub fn reference_root(
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

pub fn existence(
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

pub fn exist_cp(e: ExistenceProof) -> CommitmentProof {
    CommitmentProof {
        proof: Some(Proof::Exist(e)),
    }
}

pub fn nonexist_cp(ne: NonExistenceProof) -> CommitmentProof {
    CommitmentProof {
        proof: Some(Proof::Nonexist(ne)),
    }
}

pub fn merkle_bytes(cps: &[CommitmentProof]) -> Vec<u8> {
    let mut out = Vec::new();
    for cp in cps {
        let encoded = cp.encode_to_vec();
        out.push(0x0a);
        out.extend_from_slice(&varint(encoded.len() as u64));
        out.extend_from_slice(&encoded);
    }

    out
}

pub const VERSION: i64 = 5;

pub fn zigzag(n: i64) -> Vec<u8> {
    varint(((n << 1) ^ (n >> 63)) as u64)
}

pub fn iavl_leaf_prefix() -> Vec<u8> {
    [zigzag(0), zigzag(1), zigzag(VERSION)].concat()
}

pub fn iavl_inner(
    height: i64,
    size: i64,
    sibling: [u8; 32],
    we_are_left: bool,
) -> (Vec<u8>, Vec<u8>) {
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

pub fn hash_inner(prefix: &[u8], child: &[u8; 32], suffix: &[u8]) -> [u8; 32] {
    sha(&[prefix, child, suffix].concat())
}

pub struct IavlTree {
    pub root: [u8; 32],
    pub proofs: Vec<ExistenceProof>,
}

pub fn iavl_tree(leaves: &[(&[u8], &[u8]); 4]) -> IavlTree {
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

pub struct Multistore {
    pub root: [u8; 32],
    pub proof: ExistenceProof,
}

pub fn multistore(ibc_root: [u8; 32]) -> Multistore {
    let bank = reference_root(b"bank", &[0x42u8; 32], &[0x00], &[]);
    let inners = vec![([vec![0x01u8], bank.to_vec()].concat(), Vec::new())];

    Multistore {
        root: reference_root(STORE, &ibc_root, &[0x00], &inners),
        proof: existence(STORE, &ibc_root, &[0x00], &inners),
    }
}

pub fn receipt_key(sequence: u8) -> Vec<u8> {
    let mut key = b"10-stellar-0\x02\x00\x00\x00\x00\x00\x00\x00".to_vec();
    key.push(sequence);

    key
}

pub fn store_keys(env: &Env, key: &[u8]) -> SorobanVec<Bytes> {
    keys(env, &[STORE, key])
}

pub struct Receipts {
    pub keys: [Vec<u8>; 4],
    pub tree: IavlTree,
    pub multistore: Multistore,
}

pub fn receipts() -> Receipts {
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

pub fn absence_proof(
    r: &Receipts,
    key: &[u8],
    left: Option<usize>,
    right: Option<usize>,
) -> Vec<u8> {
    let ne = NonExistenceProof {
        key: key.to_vec(),
        left: left.map(|i| r.tree.proofs[i].clone()),
        right: right.map(|i| r.tree.proofs[i].clone()),
    };

    merkle_bytes(&[nonexist_cp(ne), exist_cp(r.multistore.proof.clone())])
}

pub fn keys(env: &Env, segments: &[&[u8]]) -> SorobanVec<Bytes> {
    let mut out = SorobanVec::new(env);

    for segment in segments {
        out.push_back(Bytes::from_slice(env, segment));
    }

    out
}

pub fn b(env: &Env, data: &[u8]) -> Bytes {
    Bytes::from_slice(env, data)
}

pub struct MembershipFixture {
    pub proof: Vec<u8>,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub root: [u8; 32],
}

pub fn membership_fixture() -> MembershipFixture {
    let r = receipts();

    MembershipFixture {
        proof: merkle_bytes(&[
            exist_cp(r.tree.proofs[2].clone()),
            exist_cp(r.multistore.proof.clone()),
        ]),
        key: r.keys[2].clone(),
        value: vec![0x01],
        root: r.multistore.root,
    }
}

pub struct NonMembershipFixture {
    pub proof: Vec<u8>,
    pub key: Vec<u8>,
    pub root: [u8; 32],
}

pub fn non_membership_fixture() -> NonMembershipFixture {
    let r = receipts();
    let absent = receipt_key(4);

    NonMembershipFixture {
        proof: absence_proof(&r, &absent, Some(1), Some(2)),
        key: absent,
        root: r.multistore.root,
    }
}

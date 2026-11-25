#![no_std]

use soroban_sdk::{Bytes, Env, Vec};

const HASH_OP_NO_HASH: u64 = 0;
const HASH_OP_SHA256: u64 = 1;
const LENGTH_OP_VAR_PROTO: u64 = 1;

const WIRE_VARINT: u8 = 0;
const WIRE_FIXED64: u8 = 1;
const WIRE_LEN: u8 = 2;
const WIRE_FIXED32: u8 = 5;

const KIND_EXIST: u8 = 1;
const KIND_NONEXIST: u8 = 2;

const LEAF_PREFIX: u8 = 0x00;
pub const PROOF_LEVELS: u32 = 2;

struct ProofSpec {
    child_size: u32,
    min_prefix_length: u32,
    max_prefix_length: u32,
    iavl: bool,
}

const IAVL_SPEC: ProofSpec = ProofSpec {
    child_size: 33,
    min_prefix_length: 4,
    max_prefix_length: 12,
    iavl: true,
};

const TENDERMINT_SPEC: ProofSpec = ProofSpec {
    child_size: 32,
    min_prefix_length: 1,
    max_prefix_length: 1,
    iavl: false,
};

#[derive(Clone, Copy, PartialEq)]
enum Branch {
    Left,
    Right,
}

struct Existence {
    root: [u8; 32],
    path: Vec<(Bytes, Bytes)>,
}

fn spec_for_level(level: u32) -> &'static ProofSpec {
    if level == 0 {
        &IAVL_SPEC
    } else {
        &TENDERMINT_SPEC
    }
}

pub fn verify_membership(
    env: &Env,
    root: &[u8; 32],
    proof: &Bytes,
    keys: &Vec<Bytes>,
    value: &Bytes,
) -> bool {
    let proofs = match collect_commitment_proofs(env, proof) {
        Some(p) if p.len() == PROOF_LEVELS && keys.len() == PROOF_LEVELS => p,
        _ => return false,
    };

    let mut current: Option<[u8; 32]> = None;

    for i in 0..proofs.len() {
        let cp = proofs.get(i).unwrap();
        let (kind, sub) = match commitment_submessage(&cp) {
            Some(v) => v,
            None => return false,
        };
        if kind != KIND_EXIST || !key_at_level(&sub, keys, i) {
            return false;
        }
        let expected = if i == 0 {
            value.clone()
        } else {
            Bytes::from_slice(env, &current.unwrap())
        };
        let stored = match message_field(&sub, 2) {
            Some(v) => v,
            None => return false,
        };
        if stored != expected {
            return false;
        }
        current = match existence_root(env, &sub, spec_for_level(i)) {
            Some(existence) => Some(existence.root),
            None => return false,
        };
    }

    current.unwrap() == *root
}

pub fn verify_non_membership(env: &Env, root: &[u8; 32], proof: &Bytes, keys: &Vec<Bytes>) -> bool {
    let proofs = match collect_commitment_proofs(env, proof) {
        Some(p) if p.len() == PROOF_LEVELS && keys.len() == PROOF_LEVELS => p,
        _ => return false,
    };

    let mut current: Option<[u8; 32]> = None;

    for i in 0..proofs.len() {
        let cp = proofs.get(i).unwrap();
        let (kind, sub) = match commitment_submessage(&cp) {
            Some(v) => v,
            None => return false,
        };
        if !key_at_level(&sub, keys, i) {
            return false;
        }
        if i == 0 {
            if kind != KIND_NONEXIST {
                return false;
            }
            current = match non_existence_root(env, &sub, spec_for_level(i)) {
                Some(r) => Some(r),
                None => return false,
            };
        } else {
            if kind != KIND_EXIST {
                return false;
            }
            let stored = match message_field(&sub, 2) {
                Some(v) => v,
                None => return false,
            };
            if stored != Bytes::from_slice(env, &current.unwrap()) {
                return false;
            }
            current = match existence_root(env, &sub, spec_for_level(i)) {
                Some(existence) => Some(existence.root),
                None => return false,
            };
        }
    }

    current.unwrap() == *root
}

fn key_at_level(sub: &Bytes, keys: &Vec<Bytes>, level: u32) -> bool {
    let expected = keys.get(keys.len() - 1 - level).unwrap();

    message_field(sub, 1) == Some(expected)
}

fn non_existence_root(env: &Env, ne: &Bytes, spec: &ProofSpec) -> Option<[u8; 32]> {
    let key = message_field(ne, 1)?;

    let left = match message_field(ne, 2) {
        Some(left) => {
            if !less_than(&message_field(&left, 1)?, &key) {
                return None;
            }

            Some(existence_root(env, &left, spec)?)
        }
        None => None,
    };

    let right = match message_field(ne, 3) {
        Some(right) => {
            if !less_than(&key, &message_field(&right, 1)?) {
                return None;
            }

            Some(existence_root(env, &right, spec)?)
        }
        None => None,
    };

    match (left, right) {
        (Some(left), None) => {
            every_step_is(spec, &left.path, left.path.len(), Branch::Right).then_some(left.root)
        }
        (None, Some(right)) => {
            every_step_is(spec, &right.path, right.path.len(), Branch::Left).then_some(right.root)
        }
        (Some(left), Some(right)) => (left.root == right.root
            && is_left_neighbor(spec, &left.path, &right.path))
        .then_some(left.root),
        (None, None) => None,
    }
}

fn branch_of(spec: &ProofSpec, prefix: &Bytes, suffix: &Bytes) -> Option<Branch> {
    let prefix_length = prefix.len();
    let suffix_length = suffix.len();

    if prefix_length >= spec.min_prefix_length
        && prefix_length <= spec.max_prefix_length
        && suffix_length == spec.child_size
    {
        return Some(Branch::Left);
    }

    if prefix_length >= spec.child_size + spec.min_prefix_length
        && prefix_length <= spec.child_size + spec.max_prefix_length
        && suffix_length == 0
    {
        return Some(Branch::Right);
    }

    None
}

fn every_step_is(spec: &ProofSpec, path: &Vec<(Bytes, Bytes)>, count: u32, branch: Branch) -> bool {
    for i in 0..count {
        let (prefix, suffix) = path.get(i).unwrap();
        if branch_of(spec, &prefix, &suffix) != Some(branch) {
            return false;
        }
    }

    true
}

fn is_left_neighbor(
    spec: &ProofSpec,
    left: &Vec<(Bytes, Bytes)>,
    right: &Vec<(Bytes, Bytes)>,
) -> bool {
    let mut left_remaining = left.len();
    let mut right_remaining = right.len();

    loop {
        if left_remaining == 0 || right_remaining == 0 {
            return false;
        }

        left_remaining -= 1;
        right_remaining -= 1;

        let (left_prefix, left_suffix) = left.get(left_remaining).unwrap();
        let (right_prefix, right_suffix) = right.get(right_remaining).unwrap();

        if left_prefix == right_prefix && left_suffix == right_suffix {
            continue;
        }

        return branch_of(spec, &left_prefix, &left_suffix) == Some(Branch::Left)
            && branch_of(spec, &right_prefix, &right_suffix) == Some(Branch::Right)
            && every_step_is(spec, left, left_remaining, Branch::Right)
            && every_step_is(spec, right, right_remaining, Branch::Left);
    }
}

#[allow(clippy::manual_is_multiple_of)]
fn inner_is_valid(spec: &ProofSpec, prefix: &Bytes, suffix: &Bytes, height: i64) -> bool {
    if prefix.get(0) == Some(LEAF_PREFIX) {
        return false;
    }

    if prefix.len() < spec.min_prefix_length
        || prefix.len() > spec.max_prefix_length + spec.child_size
        || suffix.len() % spec.child_size != 0
    {
        return false;
    }

    if spec.iavl {
        return matches!(iavl_prefix_remaining(prefix, height), Some(1) | Some(34));
    }

    true
}

fn iavl_prefix_remaining(prefix: &Bytes, min_height: i64) -> Option<u32> {
    let mut pos = 0;

    let height = zigzag(read_varint(prefix, &mut pos)?);
    let size = zigzag(read_varint(prefix, &mut pos)?);
    let version = zigzag(read_varint(prefix, &mut pos)?);

    if height < min_height || size < 0 || version < 0 {
        return None;
    }

    Some(prefix.len() - pos)
}

fn zigzag(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

fn existence_root(env: &Env, exist: &Bytes, spec: &ProofSpec) -> Option<Existence> {
    let key = message_field(exist, 1)?;
    let value = message_field(exist, 2)?;
    let leaf = message_field(exist, 3)?;
    if key.is_empty() || value.is_empty() {
        return None;
    }

    let hash = varint_field(&leaf, 1).unwrap_or(0);
    let prehash_key = varint_field(&leaf, 2).unwrap_or(0);
    let prehash_value = varint_field(&leaf, 3).unwrap_or(0);
    let length = varint_field(&leaf, 4).unwrap_or(0);
    if hash != HASH_OP_SHA256
        || prehash_key != HASH_OP_NO_HASH
        || prehash_value != HASH_OP_SHA256
        || length != LENGTH_OP_VAR_PROTO
    {
        return None;
    }
    let prefix = message_field(&leaf, 5).unwrap_or_else(|| Bytes::new(env));
    if prefix.get(0) != Some(LEAF_PREFIX) {
        return None;
    }
    if spec.iavl && iavl_prefix_remaining(&prefix, 0) != Some(0) {
        return None;
    }

    let mut pre = Bytes::new(env);
    pre.append(&prefix);
    append_varint(&mut pre, key.len() as u64);
    pre.append(&key);
    let value_hash = env.crypto().sha256(&value).to_bytes().to_array();
    append_varint(&mut pre, value_hash.len() as u64);
    pre.append(&Bytes::from_slice(env, &value_hash));
    let mut acc = env.crypto().sha256(&pre).to_bytes().to_array();

    let mut path: Vec<(Bytes, Bytes)> = Vec::new(env);
    let mut pos = 0;
    while pos < exist.len() {
        let (field, wire) = read_tag(exist, &mut pos)?;
        if field == 4 && wire == WIRE_LEN {
            let inner = read_len_delim(exist, &mut pos)?;
            let inner_hash = varint_field(&inner, 1).unwrap_or(0);
            if inner_hash != HASH_OP_SHA256 {
                return None;
            }
            let inner_prefix = message_field(&inner, 2).unwrap_or_else(|| Bytes::new(env));
            let inner_suffix = message_field(&inner, 3).unwrap_or_else(|| Bytes::new(env));
            if !inner_is_valid(spec, &inner_prefix, &inner_suffix, path.len() as i64 + 1) {
                return None;
            }

            let mut d = Bytes::new(env);
            d.append(&inner_prefix);
            d.append(&Bytes::from_slice(env, &acc));
            d.append(&inner_suffix);
            acc = env.crypto().sha256(&d).to_bytes().to_array();
            path.push_back((inner_prefix, inner_suffix));
        } else {
            skip_field(exist, &mut pos, wire)?;
        }
    }

    Some(Existence { root: acc, path })
}

fn collect_commitment_proofs(env: &Env, buf: &Bytes) -> Option<Vec<Bytes>> {
    let mut proofs: Vec<Bytes> = Vec::new(env);
    let mut pos = 0;

    while pos < buf.len() {
        let (field, wire) = read_tag(buf, &mut pos)?;
        if field == 1 && wire == WIRE_LEN {
            proofs.push_back(read_len_delim(buf, &mut pos)?);
        } else {
            skip_field(buf, &mut pos, wire)?;
        }
    }

    Some(proofs)
}

fn commitment_submessage(cp: &Bytes) -> Option<(u8, Bytes)> {
    let mut pos = 0;

    while pos < cp.len() {
        let (field, wire) = read_tag(cp, &mut pos)?;
        match (field, wire) {
            (1, WIRE_LEN) => return Some((KIND_EXIST, read_len_delim(cp, &mut pos)?)),
            (2, WIRE_LEN) => return Some((KIND_NONEXIST, read_len_delim(cp, &mut pos)?)),
            _ => skip_field(cp, &mut pos, wire)?,
        }
    }

    None
}

fn message_field(msg: &Bytes, target: u64) -> Option<Bytes> {
    let mut pos = 0;

    while pos < msg.len() {
        let (field, wire) = read_tag(msg, &mut pos)?;
        if field == target && wire == WIRE_LEN {
            return read_len_delim(msg, &mut pos);
        }

        skip_field(msg, &mut pos, wire)?;
    }

    None
}

fn varint_field(msg: &Bytes, target: u64) -> Option<u64> {
    let mut pos = 0;

    while pos < msg.len() {
        let (field, wire) = read_tag(msg, &mut pos)?;
        if field == target && wire == WIRE_VARINT {
            return read_varint(msg, &mut pos);
        }

        skip_field(msg, &mut pos, wire)?;
    }

    None
}

fn less_than(a: &Bytes, b: &Bytes) -> bool {
    let n = if a.len() < b.len() { a.len() } else { b.len() };
    let mut i = 0;
    while i < n {
        let x = a.get(i).unwrap();
        let y = b.get(i).unwrap();
        if x < y {
            return true;
        }
        if x > y {
            return false;
        }
        i += 1;
    }

    a.len() < b.len()
}

fn append_varint(out: &mut Bytes, mut n: u64) {
    loop {
        let mut byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        out.push_back(byte);
        if n == 0 {
            break;
        }
    }
}

fn read_tag(buf: &Bytes, pos: &mut u32) -> Option<(u64, u8)> {
    let key = read_varint(buf, pos)?;

    Some((key >> 3, (key & 0x07) as u8))
}

fn read_varint(buf: &Bytes, pos: &mut u32) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        if *pos >= buf.len() || shift >= 64 {
            return None;
        }
        let byte = buf.get(*pos)?;
        *pos += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Some(result)
}

fn read_len_delim(buf: &Bytes, pos: &mut u32) -> Option<Bytes> {
    let len = read_varint(buf, pos)? as u32;
    let end = pos.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let slice = buf.slice(*pos..end);
    *pos = end;

    Some(slice)
}

fn skip_field(buf: &Bytes, pos: &mut u32, wire: u8) -> Option<()> {
    match wire {
        WIRE_VARINT => {
            read_varint(buf, pos)?;
        }
        WIRE_FIXED64 => {
            let end = pos.checked_add(8)?;
            if end > buf.len() {
                return None;
            }
            *pos = end;
        }
        WIRE_LEN => {
            read_len_delim(buf, pos)?;
        }
        WIRE_FIXED32 => {
            let end = pos.checked_add(4)?;
            if end > buf.len() {
                return None;
            }
            *pos = end;
        }
        _ => return None,
    }

    Some(())
}

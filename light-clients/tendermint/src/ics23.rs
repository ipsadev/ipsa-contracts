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

pub fn verify_membership(
    env: &Env,
    root: &[u8; 32],
    proof: &Bytes,
    path: &Bytes,
    value: &Bytes,
) -> bool {
    let proofs = match collect_commitment_proofs(env, proof) {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };

    let mut current: Option<[u8; 32]> = None;

    for i in 0..proofs.len() {
        let cp = proofs.get(i).unwrap();
        let (kind, sub) = match commitment_submessage(&cp) {
            Some(v) => v,
            None => return false,
        };
        if kind != KIND_EXIST {
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
        current = match existence_root(env, &sub) {
            Some(r) => Some(r),
            None => return false,
        };
    }

    if current.unwrap() != *root {
        return false;
    }

    keys_match(env, &proofs, path)
}

pub fn verify_non_membership(env: &Env, root: &[u8; 32], proof: &Bytes, path: &Bytes) -> bool {
    let proofs = match collect_commitment_proofs(env, proof) {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };

    let mut current: Option<[u8; 32]> = None;

    for i in 0..proofs.len() {
        let cp = proofs.get(i).unwrap();
        let (kind, sub) = match commitment_submessage(&cp) {
            Some(v) => v,
            None => return false,
        };
        if i == 0 {
            if kind != KIND_NONEXIST {
                return false;
            }
            current = match non_existence_root(env, &sub) {
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
            current = match existence_root(env, &sub) {
                Some(r) => Some(r),
                None => return false,
            };
        }
    }

    if current.unwrap() != *root {
        return false;
    }

    keys_match(env, &proofs, path)
}

fn keys_match(env: &Env, proofs: &Vec<Bytes>, path: &Bytes) -> bool {
    let mut expected = Bytes::new(env);

    let mut i = proofs.len();
    while i > 0 {
        i -= 1;
        let cp = proofs.get(i).unwrap();
        let sub = match commitment_submessage(&cp) {
            Some((_, sub)) => sub,
            None => return false,
        };
        let key = match message_field(&sub, 1) {
            Some(k) => k,
            None => return false,
        };
        expected.append(&key);
    }

    &expected == path
}

fn non_existence_root(env: &Env, ne: &Bytes) -> Option<[u8; 32]> {
    let key = message_field(ne, 1)?;
    let left = message_field(ne, 2);
    let right = message_field(ne, 3);

    let mut root: Option<[u8; 32]> = None;

    if let Some(left) = left {
        let left_key = message_field(&left, 1)?;
        if !less_than(&left_key, &key) {
            return None;
        }
        root = Some(existence_root(env, &left)?);
    }

    if let Some(right) = right {
        let right_key = message_field(&right, 1)?;
        if !less_than(&key, &right_key) {
            return None;
        }
        let r = existence_root(env, &right)?;
        if let Some(l) = root {
            if l != r {
                return None;
            }
        }
        root = Some(r);
    }

    root
}

fn existence_root(env: &Env, exist: &Bytes) -> Option<[u8; 32]> {
    let key = message_field(exist, 1)?;
    let value = message_field(exist, 2)?;
    let leaf = message_field(exist, 3)?;

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

    let mut pre = Bytes::new(env);
    pre.append(&prefix);
    append_varint(&mut pre, key.len() as u64);
    pre.append(&key);
    let value_hash = env.crypto().sha256(&value).to_bytes().to_array();
    append_varint(&mut pre, value_hash.len() as u64);
    pre.append(&Bytes::from_slice(env, &value_hash));
    let mut acc = env.crypto().sha256(&pre).to_bytes().to_array();

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

            let mut d = Bytes::new(env);
            d.append(&inner_prefix);
            d.append(&Bytes::from_slice(env, &acc));
            d.append(&inner_suffix);
            acc = env.crypto().sha256(&d).to_bytes().to_array();
        } else {
            skip_field(exist, &mut pos, wire)?;
        }
    }

    Some(acc)
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

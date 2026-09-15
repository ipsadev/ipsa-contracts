use soroban_sdk::{Bytes, Env, Vec};

pub const WIRE_VARINT: u8 = 0;
pub const WIRE_FIXED64: u8 = 1;
pub const WIRE_LEN: u8 = 2;
pub const WIRE_FIXED32: u8 = 5;

pub fn read_tag(buf: &Bytes, pos: &mut u32) -> Option<(u64, u8)> {
    let key = read_varint(buf, pos)?;

    Some((key >> 3, (key & 0x07) as u8))
}

pub fn read_varint(buf: &Bytes, pos: &mut u32) -> Option<u64> {
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

pub fn read_len_delim(buf: &Bytes, pos: &mut u32) -> Option<Bytes> {
    let len = read_varint(buf, pos)? as u32;
    let end = pos.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let slice = buf.slice(*pos..end);
    *pos = end;

    Some(slice)
}

pub fn skip_field(buf: &Bytes, pos: &mut u32, wire: u8) -> Option<()> {
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

pub fn message_field(msg: &Bytes, target: u64) -> Option<Bytes> {
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

pub fn varint_field(msg: &Bytes, target: u64) -> Option<u64> {
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

pub fn repeated_field(env: &Env, msg: &Bytes, target: u64) -> Option<Vec<Bytes>> {
    let mut out: Vec<Bytes> = Vec::new(env);
    let mut pos = 0;

    while pos < msg.len() {
        let (field, wire) = read_tag(msg, &mut pos)?;
        if field == target && wire == WIRE_LEN {
            out.push_back(read_len_delim(msg, &mut pos)?);
        } else {
            skip_field(msg, &mut pos, wire)?;
        }
    }

    Some(out)
}

pub fn append_varint(out: &mut Bytes, mut n: u64) {
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

pub fn append_len_delimited(out: &mut Bytes, tag: u8, data: &Bytes) {
    out.push_back(tag);
    append_varint(out, data.len() as u64);
    out.append(data);
}

pub fn append_fixed64_le(out: &mut Bytes, value: u64) {
    let mut i = 0;
    while i < 8 {
        out.push_back(((value >> (i * 8)) & 0xff) as u8);
        i += 1;
    }
}

pub fn to_array<const N: usize>(bytes: &Bytes) -> Option<[u8; N]> {
    if bytes.len() != N as u32 {
        return None;
    }
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = bytes.get(i as u32)?;
        i += 1;
    }

    Some(out)
}

pub fn sha256(env: &Env, data: &Bytes) -> [u8; 32] {
    env.crypto().sha256(data).to_bytes().to_array()
}

pub fn leaf_hash(env: &Env, item: &Bytes) -> [u8; 32] {
    let mut buf = Bytes::from_slice(env, &[0x00]);
    buf.append(item);

    sha256(env, &buf)
}

pub fn inner_hash(env: &Env, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = Bytes::from_slice(env, &[0x01]);
    buf.append(&Bytes::from_slice(env, left));
    buf.append(&Bytes::from_slice(env, right));

    sha256(env, &buf)
}

pub fn merkle_root(env: &Env, items: &Vec<Bytes>) -> [u8; 32] {
    merkle_range(env, items, 0, items.len())
}

fn merkle_range(env: &Env, items: &Vec<Bytes>, lo: u32, hi: u32) -> [u8; 32] {
    let n = hi - lo;
    if n == 0 {
        return sha256(env, &Bytes::new(env));
    }
    if n == 1 {
        return leaf_hash(env, &items.get(lo).unwrap());
    }

    let k = largest_power_of_two_below(n);
    let left = merkle_range(env, items, lo, lo + k);
    let right = merkle_range(env, items, lo + k, hi);

    inner_hash(env, &left, &right)
}

fn largest_power_of_two_below(n: u32) -> u32 {
    let mut k = 1;
    while k << 1 < n {
        k <<= 1;
    }

    k
}

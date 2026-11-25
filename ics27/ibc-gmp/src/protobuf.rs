use soroban_sdk::{panic_with_error, Bytes, Env, String};

use crate::{errors::Error, types::GmpPacketData};

const LENGTH_DELIMITED: u32 = 2;

const MAX_STRING_BYTES: usize = 4_096;

pub fn encode_packet(env: &Env, packet: &GmpPacketData) -> Bytes {
    let mut out = Bytes::new(env);

    append_field(&mut out, 1, &packet.sender.to_bytes());
    append_field(&mut out, 2, &packet.receiver.to_bytes());
    append_field(&mut out, 3, &packet.salt);
    append_field(&mut out, 4, &packet.payload);
    append_field(&mut out, 5, &packet.memo.to_bytes());

    out
}

pub fn decode_packet(env: &Env, value: &Bytes) -> GmpPacketData {
    let mut packet = GmpPacketData {
        sender: String::from_str(env, ""),
        receiver: String::from_str(env, ""),
        salt: Bytes::new(env),
        payload: Bytes::new(env),
        memo: String::from_str(env, ""),
    };
    let mut at = 0u32;

    while at < value.len() {
        let (field, data, next) = read_field(value, at)
            .unwrap_or_else(|| panic_with_error!(env, Error::InvalidPacketData));

        match field {
            1 => packet.sender = to_string(env, &data),
            2 => packet.receiver = to_string(env, &data),
            3 => packet.salt = data,
            4 => packet.payload = data,
            5 => packet.memo = to_string(env, &data),
            _ => panic_with_error!(env, Error::InvalidPacketData),
        }

        at = next;
    }

    packet
}

pub fn encode_acknowledgement(env: &Env, result: &Bytes) -> Bytes {
    let mut out = Bytes::new(env);

    append_field(&mut out, 1, result);

    out
}

pub fn decode_acknowledgement(env: &Env, value: &Bytes) -> Option<Bytes> {
    let mut result = Bytes::new(env);
    let mut at = 0u32;

    while at < value.len() {
        let (field, data, next) = read_field(value, at)?;

        if field != 1 {
            return None;
        }

        result = data;
        at = next;
    }

    Some(result)
}

fn append_field(out: &mut Bytes, field: u32, data: &Bytes) {
    if data.is_empty() {
        return;
    }

    append_varint(out, (field << 3) | LENGTH_DELIMITED);
    append_varint(out, data.len());
    out.append(data);
}

fn append_varint(out: &mut Bytes, mut value: u32) {
    while value >= 0x80 {
        out.push_back((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }

    out.push_back(value as u8);
}

fn read_field(value: &Bytes, at: u32) -> Option<(u32, Bytes, u32)> {
    let (tag, at) = read_varint(value, at)?;

    if tag & 0x07 != LENGTH_DELIMITED {
        return None;
    }

    let (length, start) = read_varint(value, at)?;
    let end = start.checked_add(length)?;

    if end > value.len() {
        return None;
    }

    Some((tag >> 3, value.slice(start..end), end))
}

fn read_varint(value: &Bytes, mut at: u32) -> Option<(u32, u32)> {
    let mut result = 0u32;

    for shift in [0u32, 7, 14, 21, 28] {
        let byte = value.get(at)?;
        at += 1;

        let bits = u32::from(byte & 0x7f);

        if shift == 28 && bits > 0x0f {
            return None;
        }

        result |= bits << shift;

        if byte & 0x80 == 0 {
            return Some((result, at));
        }
    }

    None
}

fn to_string(env: &Env, bytes: &Bytes) -> String {
    let length = bytes.len() as usize;

    if length > MAX_STRING_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buffer = [0u8; MAX_STRING_BYTES];
    bytes.copy_into_slice(&mut buffer[..length]);

    String::from_bytes(env, &buffer[..length])
}

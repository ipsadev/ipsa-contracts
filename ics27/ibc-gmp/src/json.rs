use soroban_sdk::{panic_with_error, Bytes, Env, String};

use crate::{errors::Error, types::GmpPacketData};

pub const MAX_JSON_BYTES: usize = 8192;

const FIELD_BYTES: usize = 6144;
const KEY_BYTES: usize = 16;

pub fn decode_packet(env: &Env, value: &Bytes) -> GmpPacketData {
    let len = value.len() as usize;

    if len == 0 || len > MAX_JSON_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_JSON_BYTES];
    value.copy_into_slice(&mut buf[..len]);
    let src = &buf[..len];

    let mut sender = String::from_str(env, "");
    let mut receiver = String::from_str(env, "");
    let mut memo = String::from_str(env, "");
    let mut salt = Bytes::new(env);
    let mut payload = Bytes::new(env);

    let mut i = skip_ws(src, 0);

    if i >= src.len() || src[i] != b'{' {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    i += 1;

    loop {
        i = skip_ws(src, i);

        if i < src.len() && src[i] == b'}' {
            break;
        }

        let mut key = [0u8; KEY_BYTES];
        let (key_len, next) = parse_string(env, src, i, &mut key);
        i = skip_ws(src, next);

        if i >= src.len() || src[i] != b':' {
            panic_with_error!(env, Error::InvalidPacketData);
        }

        i = skip_ws(src, i + 1);

        let mut val = [0u8; FIELD_BYTES];
        let (val_len, next) = parse_string(env, src, i, &mut val);
        i = next;

        let k = &key[..key_len];
        let v = &val[..val_len];

        if k == b"sender" {
            sender = String::from_bytes(env, v);
        } else if k == b"receiver" {
            receiver = String::from_bytes(env, v);
        } else if k == b"memo" {
            memo = String::from_bytes(env, v);
        } else if k == b"salt" {
            salt = base64_decode(env, v);
        } else if k == b"payload" {
            payload = base64_decode(env, v);
        }

        i = skip_ws(src, i);

        if i < src.len() && src[i] == b',' {
            i += 1;
        } else if i < src.len() && src[i] == b'}' {
            break;
        } else {
            panic_with_error!(env, Error::InvalidPacketData);
        }
    }

    GmpPacketData {
        sender,
        receiver,
        salt,
        payload,
        memo,
    }
}

pub fn encode_packet(env: &Env, packet: &GmpPacketData) -> Bytes {
    let mut out = Bytes::new(env);

    append(env, &mut out, b"{\"sender\":\"");
    append_string(env, &mut out, &packet.sender);
    append(env, &mut out, b"\",\"receiver\":\"");
    append_string(env, &mut out, &packet.receiver);
    append(env, &mut out, b"\",\"salt\":\"");
    append_base64(env, &mut out, &packet.salt);
    append(env, &mut out, b"\",\"payload\":\"");
    append_base64(env, &mut out, &packet.payload);
    append(env, &mut out, b"\",\"memo\":\"");
    append_string(env, &mut out, &packet.memo);
    append(env, &mut out, b"\"}");

    out
}

fn skip_ws(src: &[u8], mut i: usize) -> usize {
    while i < src.len() {
        match src[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            _ => break,
        }
    }

    i
}

fn parse_string(env: &Env, src: &[u8], start: usize, out: &mut [u8]) -> (usize, usize) {
    let mut i = start;

    if i >= src.len() || src[i] != b'"' {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    i += 1;

    let mut written = 0usize;

    while i < src.len() {
        let byte = src[i];

        if byte == b'"' {
            return (written, i + 1);
        }

        if byte == b'\\' {
            i += 1;

            if i >= src.len() {
                panic_with_error!(env, Error::InvalidPacketData);
            }

            let escaped = match src[i] {
                b'"' => b'"',
                b'\\' => b'\\',
                b'/' => b'/',
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                _ => panic_with_error!(env, Error::InvalidPacketData),
            };

            if written >= out.len() {
                panic_with_error!(env, Error::InvalidPacketData);
            }

            out[written] = escaped;
            written += 1;
            i += 1;

            continue;
        }

        if written >= out.len() {
            panic_with_error!(env, Error::InvalidPacketData);
        }

        out[written] = byte;
        written += 1;
        i += 1;
    }

    panic_with_error!(env, Error::InvalidPacketData)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_value(env: &Env, byte: u8) -> u32 {
    match byte {
        b'A'..=b'Z' => u32::from(byte - b'A'),
        b'a'..=b'z' => u32::from(byte - b'a') + 26,
        b'0'..=b'9' => u32::from(byte - b'0') + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic_with_error!(env, Error::InvalidPacketData),
    }
}

fn base64_decode(env: &Env, src: &[u8]) -> Bytes {
    let mut out = Bytes::new(env);

    if src.is_empty() {
        return out;
    }

    let mut acc: u32 = 0;
    let mut bits = 0u32;

    for byte in src {
        if *byte == b'=' {
            break;
        }

        acc = (acc << 6) | base64_value(env, *byte);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            out.push_back(((acc >> bits) & 0xff) as u8);
        }
    }

    out
}

fn append_base64(env: &Env, out: &mut Bytes, value: &Bytes) {
    let len = value.len();

    if len == 0 {
        return;
    }

    let mut i = 0u32;

    while i < len {
        let b0 = value.get(i).unwrap_or(0);
        let b1 = value.get(i + 1);
        let b2 = value.get(i + 2);

        out.push_back(ALPHABET[(b0 >> 2) as usize]);

        let second = ((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4);
        out.push_back(ALPHABET[second as usize]);

        match b1 {
            None => {
                out.push_back(b'=');
                out.push_back(b'=');
            }
            Some(b1) => {
                let third = ((b1 & 0x0f) << 2) | (b2.unwrap_or(0) >> 6);
                out.push_back(ALPHABET[third as usize]);

                match b2 {
                    None => out.push_back(b'='),
                    Some(b2) => out.push_back(ALPHABET[(b2 & 0x3f) as usize]),
                }
            }
        }

        let _ = env;
        i += 3;
    }
}

fn append(env: &Env, out: &mut Bytes, literal: &[u8]) {
    out.append(&Bytes::from_slice(env, literal));
}

fn append_string(env: &Env, out: &mut Bytes, value: &String) {
    let len = value.len() as usize;

    if len == 0 {
        return;
    }

    if len > FIELD_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; FIELD_BYTES];
    value.copy_into_slice(&mut buf[..len]);

    out.append(&Bytes::from_slice(env, &buf[..len]));
}

pub fn encode_acknowledgement(env: &Env, result: &Bytes) -> Bytes {
    let mut out = Bytes::new(env);

    if result.is_empty() {
        append(env, &mut out, b"{}");

        return out;
    }

    append(env, &mut out, b"{\"result\":\"");
    append_base64(env, &mut out, result);
    append(env, &mut out, b"\"}");

    out
}

pub fn decode_acknowledgement(env: &Env, value: &Bytes) -> Option<Bytes> {
    const PREFIX: &[u8] = b"{\"result\":\"";
    const SUFFIX: &[u8] = b"\"}";

    let length = value.len() as usize;

    if length > MAX_JSON_BYTES {
        return None;
    }

    let mut buffer = [0u8; MAX_JSON_BYTES];
    value.copy_into_slice(&mut buffer[..length]);
    let source = &buffer[..length];

    if source == b"{}" {
        return Some(Bytes::new(env));
    }

    if length < PREFIX.len() + SUFFIX.len()
        || !source.starts_with(PREFIX)
        || !source.ends_with(SUFFIX)
    {
        return None;
    }

    try_base64_decode(env, &source[PREFIX.len()..length - SUFFIX.len()])
}

fn try_base64_decode(env: &Env, source: &[u8]) -> Option<Bytes> {
    let mut out = Bytes::new(env);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;

    for byte in source {
        if *byte == b'=' {
            break;
        }

        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };

        accumulator = (accumulator << 6) | value;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            out.push_back(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Some(out)
}

use soroban_sdk::{panic_with_error, Bytes, Env, String};

use crate::{
    errors::Error,
    limits::{
        MAX_MEMO_BYTES, MAX_PAYLOAD_BYTES, MAX_RECEIVER_BYTES, MAX_SALT_BYTES, MAX_SENDER_BYTES,
    },
    types::GmpPacketData,
};

const WORD: u32 = 32;

const PACKET_FIELDS: u32 = 5;

const MAX_STRING_BYTES: usize = MAX_MEMO_BYTES as usize;

pub fn encode_packet(env: &Env, packet: &GmpPacketData) -> Bytes {
    let fields = [
        packet.sender.to_bytes(),
        packet.receiver.to_bytes(),
        packet.salt.clone(),
        packet.payload.clone(),
        packet.memo.to_bytes(),
    ];

    let mut head = Bytes::new(env);
    let mut tails = Bytes::new(env);

    for field in fields.iter() {
        head.append(&word(env, PACKET_FIELDS * WORD + tails.len()));
        append_dynamic(env, &mut tails, field);
    }

    let mut out = word(env, WORD);
    out.append(&head);
    out.append(&tails);

    out
}

pub fn decode_packet(env: &Env, value: &Bytes) -> GmpPacketData {
    if read_number(env, value, 0) != WORD {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let tuple = value.slice(WORD..);

    let sender = read_dynamic(env, &tuple, 0, MAX_SENDER_BYTES);
    let receiver = read_dynamic(env, &tuple, 1, MAX_RECEIVER_BYTES);
    let salt = read_dynamic(env, &tuple, 2, MAX_SALT_BYTES);
    let payload = read_dynamic(env, &tuple, 3, MAX_PAYLOAD_BYTES);
    let memo = read_dynamic(env, &tuple, 4, MAX_MEMO_BYTES);

    GmpPacketData {
        sender: to_string(env, &sender),
        receiver: to_string(env, &receiver),
        salt,
        payload,
        memo: to_string(env, &memo),
    }
}

pub fn encode_acknowledgement(env: &Env, result: &Bytes) -> Bytes {
    let mut out = word(env, WORD);
    out.append(&word(env, WORD));
    append_dynamic(env, &mut out, result);

    out
}

fn word(env: &Env, value: u32) -> Bytes {
    let mut bytes = [0u8; WORD as usize];
    bytes[28..].copy_from_slice(&value.to_be_bytes());

    Bytes::from_slice(env, &bytes)
}

fn append_dynamic(env: &Env, out: &mut Bytes, data: &Bytes) {
    let padding = (WORD - data.len() % WORD) % WORD;

    out.append(&word(env, data.len()));
    out.append(data);
    out.append(&Bytes::from_slice(
        env,
        &[0u8; WORD as usize][..padding as usize],
    ));
}

fn read_number(env: &Env, source: &Bytes, at: u32) -> u32 {
    if at.checked_add(WORD).is_none_or(|end| end > source.len()) {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut bytes = [0u8; WORD as usize];
    source.slice(at..at + WORD).copy_into_slice(&mut bytes);

    if bytes[..28].iter().any(|byte| *byte != 0) {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    u32::from_be_bytes([bytes[28], bytes[29], bytes[30], bytes[31]])
}

fn read_dynamic(env: &Env, tuple: &Bytes, field: u32, max: u32) -> Bytes {
    let offset = read_number(env, tuple, field * WORD);
    let length = read_number(env, tuple, offset);
    let start = offset + WORD;

    if length > max
        || start
            .checked_add(length)
            .is_none_or(|end| end > tuple.len())
    {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    tuple.slice(start..start + length)
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

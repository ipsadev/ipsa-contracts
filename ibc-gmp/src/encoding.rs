use soroban_sdk::{panic_with_error, xdr::FromXdr, Bytes, Env, String};

use crate::{
    abi,
    errors::Error,
    json, protobuf,
    types::{Acknowledgement, GmpCall, GmpPacketData},
    ENCODING, ENCODING_JSON, ENCODING_PROTOBUF, ENCODING_SOLIDITY_ABI,
};

pub fn decode_packet(env: &Env, encoding: &String, value: &Bytes) -> GmpPacketData {
    if *encoding == String::from_str(env, ENCODING) {
        return GmpPacketData::from_xdr(env, value)
            .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidPacketData));
    }

    if *encoding == String::from_str(env, ENCODING_JSON) {
        return json::decode_packet(env, value);
    }

    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return abi::decode_packet(env, value);
    }

    if *encoding == String::from_str(env, ENCODING_PROTOBUF) {
        return protobuf::decode_packet(env, value);
    }

    panic_with_error!(env, Error::UnsupportedEncoding)
}

pub fn encode_packet(env: &Env, encoding: &String, packet: &GmpPacketData) -> Bytes {
    if *encoding == String::from_str(env, ENCODING) {
        return soroban_sdk::xdr::ToXdr::to_xdr(packet.clone(), env);
    }

    if *encoding == String::from_str(env, ENCODING_JSON) {
        return json::encode_packet(env, packet);
    }

    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return abi::encode_packet(env, packet);
    }

    if *encoding == String::from_str(env, ENCODING_PROTOBUF) {
        return protobuf::encode_packet(env, packet);
    }

    panic_with_error!(env, Error::UnsupportedEncoding)
}

pub fn encode_acknowledgement(env: &Env, encoding: &String, result: Bytes) -> Bytes {
    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return abi::encode_acknowledgement(env, &result);
    }

    if *encoding == String::from_str(env, ENCODING_JSON) {
        return json::encode_acknowledgement(env, &result);
    }

    if *encoding == String::from_str(env, ENCODING_PROTOBUF) {
        return protobuf::encode_acknowledgement(env, &result);
    }

    soroban_sdk::xdr::ToXdr::to_xdr(Acknowledgement { result }, env)
}

pub fn decode_acknowledgement(env: &Env, encoding: &String, value: &Bytes) -> Option<Bytes> {
    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return abi::decode_acknowledgement(value);
    }

    if *encoding == String::from_str(env, ENCODING_JSON) {
        return json::decode_acknowledgement(env, value);
    }

    if *encoding == String::from_str(env, ENCODING_PROTOBUF) {
        return protobuf::decode_acknowledgement(env, value);
    }

    if *encoding == String::from_str(env, ENCODING) {
        return decode_xdr_acknowledgement(value);
    }

    None
}

const XDR_ACKNOWLEDGEMENT_PREFIX: [u8; 32] = [
    0, 0, 0, 0x11, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0x0f, 0, 0, 0, 6, b'r', b'e', b's', b'u', b'l',
    b't', 0, 0, 0, 0, 0, 0x0d,
];

fn decode_xdr_acknowledgement(value: &Bytes) -> Option<Bytes> {
    let prefix_length = XDR_ACKNOWLEDGEMENT_PREFIX.len() as u32;
    let start = prefix_length + 4;

    if value.len() < start {
        return None;
    }

    let mut prefix = [0u8; XDR_ACKNOWLEDGEMENT_PREFIX.len()];
    value.slice(0..prefix_length).copy_into_slice(&mut prefix);

    if prefix != XDR_ACKNOWLEDGEMENT_PREFIX {
        return None;
    }

    let mut length = [0u8; 4];
    value
        .slice(prefix_length..start)
        .copy_into_slice(&mut length);

    let length = u32::from_be_bytes(length);
    let end = start.checked_add(length)?;
    let padded = end.checked_add((4 - length % 4) % 4)?;

    if padded != value.len() {
        return None;
    }

    Some(value.slice(start..end))
}

pub fn decode_call(env: &Env, payload: &Bytes) -> GmpCall {
    GmpCall::from_xdr(env, payload)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidCallPayload))
}

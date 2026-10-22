use soroban_sdk::{panic_with_error, xdr::FromXdr, Bytes, Env, String};

use crate::{
    abi,
    errors::Error,
    json,
    types::{Acknowledgement, GmpCall, GmpPacketData},
    ENCODING, ENCODING_JSON, ENCODING_SOLIDITY_ABI,
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

    panic_with_error!(env, Error::UnsupportedEncoding)
}

pub fn encode_acknowledgement(env: &Env, encoding: &String, result: Bytes) -> Bytes {
    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return abi::encode_acknowledgement(env, &result);
    }

    soroban_sdk::xdr::ToXdr::to_xdr(Acknowledgement { result }, env)
}

pub fn decode_call(env: &Env, payload: &Bytes) -> GmpCall {
    GmpCall::from_xdr(env, payload)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidCallPayload))
}

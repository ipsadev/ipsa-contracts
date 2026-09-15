use soroban_sdk::{panic_with_error, xdr::FromXdr, Bytes, Env, String};

use crate::errors::Error;
use crate::json;
use crate::types::{GmpCall, GmpPacketData};
use crate::{ENCODING, ENCODING_JSON};

pub fn decode_packet(env: &Env, encoding: &String, value: &Bytes) -> GmpPacketData {
    if *encoding == String::from_str(env, ENCODING) {
        return GmpPacketData::from_xdr(env, value)
            .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidPacketData));
    }

    if *encoding == String::from_str(env, ENCODING_JSON) {
        return json::decode_packet(env, value);
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

    panic_with_error!(env, Error::UnsupportedEncoding)
}

pub fn decode_call(env: &Env, payload: &Bytes) -> GmpCall {
    GmpCall::from_xdr(env, payload)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidCallPayload))
}

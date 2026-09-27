use soroban_sdk::{panic_with_error, Bytes, Env, String};

use crate::errors::Error;

const MAX_TRACE_BYTES: usize = 512;

pub fn received_denom(
    env: &Env,
    source_port: &String,
    source_client: &String,
    dest_port: &String,
    dest_client: &String,
    denom: &String,
) -> String {
    let denom_bytes = string_bytes(env, denom);
    let returning_prefix = denom_prefix(env, source_port, source_client);

    if starts_with(&denom_bytes, &returning_prefix) {
        let base = denom_bytes.slice(returning_prefix.len()..);

        if base.is_empty() {
            panic_with_error!(env, Error::InvalidPacketData);
        }

        return bytes_string(env, &base);
    }

    let mut prefixed = denom_prefix(env, dest_port, dest_client);

    prefixed.append(&denom_bytes);

    bytes_string(env, &prefixed)
}

fn denom_prefix(env: &Env, port: &String, client: &String) -> Bytes {
    let mut prefix = string_bytes(env, port);

    prefix.append(&Bytes::from_slice(env, b"/"));
    prefix.append(&string_bytes(env, client));
    prefix.append(&Bytes::from_slice(env, b"/"));

    prefix
}

fn starts_with(bytes: &Bytes, prefix: &Bytes) -> bool {
    bytes.len() >= prefix.len() && bytes.slice(..prefix.len()) == *prefix
}

fn string_bytes(env: &Env, s: &String) -> Bytes {
    let len = s.len() as usize;

    if len > MAX_TRACE_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_TRACE_BYTES];
    s.copy_into_slice(&mut buf[..len]);

    Bytes::from_slice(env, &buf[..len])
}

fn bytes_string(env: &Env, bytes: &Bytes) -> String {
    let len = bytes.len() as usize;

    if len > MAX_TRACE_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_TRACE_BYTES];
    bytes.copy_into_slice(&mut buf[..len]);

    String::from_bytes(env, &buf[..len])
}

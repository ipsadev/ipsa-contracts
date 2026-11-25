use soroban_sdk::{panic_with_error, vec, Address, Env, IntoVal, String, Symbol};

use crate::{errors::Error, types::TransferReceived};

const MAX_INSPECTED_MEMO_BYTES: usize = 1024;

const CALLBACK_KEY: &[u8] = b"\"dest_callback\"";

const ADDRESS_KEY: &[u8] = b"\"address\"";

const CALLBACK_FUNCTION: &str = "on_ibc_transfer_received";

pub(crate) fn destination_callback(env: &Env, memo: &String) -> Option<String> {
    let length = memo.len() as usize;

    if length == 0 || length > MAX_INSPECTED_MEMO_BYTES {
        return None;
    }

    let mut buffer = [0u8; MAX_INSPECTED_MEMO_BYTES];
    let bytes = &mut buffer[..length];
    memo.copy_into_slice(bytes);

    let key_end = find(bytes, CALLBACK_KEY, 0)? + CALLBACK_KEY.len();

    let address = parse_callback_object(bytes, key_end)
        .unwrap_or_else(|| panic_with_error!(env, Error::InvalidCallback));

    let text = core::str::from_utf8(address)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidCallback));

    Some(String::from_str(env, text))
}

pub(crate) fn call(env: &Env, target: &Address, received: TransferReceived) {
    let _: () = env.invoke_contract(
        target,
        &Symbol::new(env, CALLBACK_FUNCTION),
        vec![
            env,
            env.current_contract_address().into_val(env),
            received.into_val(env),
        ],
    );
}

fn parse_callback_object(bytes: &[u8], from: usize) -> Option<&[u8]> {
    let mut position = expect(bytes, skip_whitespace(bytes, from), b':')?;

    position = expect(bytes, skip_whitespace(bytes, position), b'{')?;

    let object_end = find(bytes, b"}", position)?;

    let key_end = find(&bytes[..object_end], ADDRESS_KEY, position)? + ADDRESS_KEY.len();

    position = expect(bytes, skip_whitespace(bytes, key_end), b':')?;

    position = expect(bytes, skip_whitespace(bytes, position), b'"')?;

    let value_end = find(&bytes[..object_end], b"\"", position)?;

    let value = &bytes[position..value_end];

    if value.is_empty() || !value.iter().all(u8::is_ascii_alphanumeric) {
        return None;
    }

    Some(value)
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= haystack.len() || needle.len() > haystack.len() - from {
        return None;
    }

    (from..=haystack.len() - needle.len())
        .find(|&index| &haystack[index..index + needle.len()] == needle)
}

fn skip_whitespace(bytes: &[u8], mut position: usize) -> usize {
    while position < bytes.len() && bytes[position].is_ascii_whitespace() {
        position += 1;
    }

    position
}

fn expect(bytes: &[u8], position: usize, expected: u8) -> Option<usize> {
    if position < bytes.len() && bytes[position] == expected {
        return Some(position + 1);
    }

    None
}

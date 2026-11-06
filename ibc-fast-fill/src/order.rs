use soroban_sdk::{panic_with_error, Address, Bytes, BytesN, Env, String};

use crate::{
    errors::Error,
    types::{Order, Terms},
};

const MAX_MEMO_BYTES: usize = 1024;

const TERMS_KEY: &[u8] = b"\"fast_fill\"";

const ADDRESS_KEY: &[u8] = b"\"address\":\"";

const ASSIGN_REFUND_SIGNATURE: &[u8] = b"assignRefund(bytes32,address)";

pub fn key(env: &Env, order: &Order) -> BytesN<32> {
    let mut preimage = Bytes::from_array(env, &order.sequence.to_be_bytes());

    preimage.append(&Bytes::from_array(env, &[0u8; 16]));
    preimage.append(&Bytes::from_array(
        env,
        &(order.amount as u128).to_be_bytes(),
    ));
    preimage.append(&digest(env, &order.source_client));
    preimage.append(&digest(env, &order.denom));
    preimage.append(&digest(env, &order.memo));

    env.crypto().sha256(&preimage).into()
}

pub fn terms(env: &Env, memo: &String) -> Terms {
    let length = memo.len() as usize;

    if length == 0 || length > MAX_MEMO_BYTES {
        panic_with_error!(env, Error::InvalidTerms);
    }

    let mut buffer = [0u8; MAX_MEMO_BYTES];
    let bytes = &mut buffer[..length];

    memo.copy_into_slice(bytes);

    let value = terms_value(bytes).unwrap_or_else(|| panic_with_error!(env, Error::InvalidTerms));

    let mut parts = value.split(|byte| *byte == b':');

    let recipient = parts.next().filter(|part| !part.is_empty());
    let fee = parts.next().and_then(decimal);
    let deadline = parts.next().and_then(decimal);

    if parts.next().is_some() {
        panic_with_error!(env, Error::InvalidTerms);
    }

    match (recipient, fee, deadline) {
        (Some(recipient), Some(fee), Some(deadline))
            if fee <= i128::MAX as u128 && deadline <= u64::MAX as u128 =>
        {
            let text = core::str::from_utf8(recipient)
                .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidTerms));

            Terms {
                recipient: Address::from_string(&String::from_str(env, text)),
                fee: fee as i128,
                deadline: deadline as u64,
            }
        }
        _ => panic_with_error!(env, Error::InvalidTerms),
    }
}

pub fn addressed_to(memo: &String, contract: &Address) -> bool {
    let length = memo.len() as usize;

    if length == 0 || length > MAX_MEMO_BYTES {
        return false;
    }

    let mut memo_buffer = [0u8; MAX_MEMO_BYTES];
    let memo_bytes = &mut memo_buffer[..length];

    memo.copy_into_slice(memo_bytes);

    let strkey = contract.to_string();
    let strkey_length = strkey.len() as usize;

    let mut needle_buffer = [0u8; 80];
    let needle_length = ADDRESS_KEY.len() + strkey_length + 1;

    if needle_length > needle_buffer.len() {
        return false;
    }

    needle_buffer[..ADDRESS_KEY.len()].copy_from_slice(ADDRESS_KEY);
    strkey
        .copy_into_slice(&mut needle_buffer[ADDRESS_KEY.len()..ADDRESS_KEY.len() + strkey_length]);
    needle_buffer[needle_length - 1] = b'"';

    find(memo_bytes, &needle_buffer[..needle_length], 0).is_some()
}

pub fn evm_address(env: &Env, address: &String) -> [u8; 20] {
    if address.len() != 42 {
        panic_with_error!(env, Error::InvalidEvmAddress);
    }

    let mut text = [0u8; 42];

    address.copy_into_slice(&mut text);

    if &text[..2] != b"0x" && &text[..2] != b"0X" {
        panic_with_error!(env, Error::InvalidEvmAddress);
    }

    let mut out = [0u8; 20];

    for (index, pair) in text[2..].chunks(2).enumerate() {
        match (nibble(pair[0]), nibble(pair[1])) {
            (Some(high), Some(low)) => out[index] = (high << 4) | low,
            _ => panic_with_error!(env, Error::InvalidEvmAddress),
        }
    }

    out
}

pub fn assign_refund_call(env: &Env, key: &BytesN<32>, provider_evm: &[u8; 20]) -> Bytes {
    let selector = env
        .crypto()
        .keccak256(&Bytes::from_slice(env, ASSIGN_REFUND_SIGNATURE))
        .to_array();

    let mut call = Bytes::from_slice(env, &selector[..4]);

    call.append(&Bytes::from_array(env, &key.to_array()));
    call.append(&Bytes::from_array(env, &[0u8; 12]));
    call.append(&Bytes::from_array(env, provider_evm));

    call
}

fn digest(env: &Env, value: &String) -> Bytes {
    let length = value.len() as usize;

    if length > MAX_MEMO_BYTES {
        panic_with_error!(env, Error::InvalidTerms);
    }

    let mut buffer = [0u8; MAX_MEMO_BYTES];

    value.copy_into_slice(&mut buffer[..length]);

    let hash: BytesN<32> = env
        .crypto()
        .sha256(&Bytes::from_slice(env, &buffer[..length]))
        .into();

    Bytes::from_array(env, &hash.to_array())
}

fn terms_value(bytes: &[u8]) -> Option<&[u8]> {
    let mut position = find(bytes, TERMS_KEY, 0)? + TERMS_KEY.len();

    position = skip_whitespace(bytes, position);

    if bytes.get(position) != Some(&b':') {
        return None;
    }

    position = skip_whitespace(bytes, position + 1);

    if bytes.get(position) != Some(&b'"') {
        return None;
    }

    let start = position + 1;
    let end = find(bytes, b"\"", start)?;

    Some(&bytes[start..end])
}

fn decimal(digits: &[u8]) -> Option<u128> {
    if digits.is_empty() || digits.len() > 38 {
        return None;
    }

    digits.iter().try_fold(0u128, |total, digit| {
        if digit.is_ascii_digit() {
            total.checked_mul(10)?.checked_add(u128::from(digit - b'0'))
        } else {
            None
        }
    })
}

fn nibble(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
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

use soroban_sdk::{panic_with_error, Address, Bytes, Env, String};

use crate::{
    errors::Error,
    types::{FungibleTokenPacketData, Token},
};

pub(crate) const SUCCESS_ACK_BYTE: u8 = 0x01;
pub(crate) const SUCCESS_ACK_JSON: &[u8] = b"{\"result\":\"AQ==\"}";

pub const ENCODING_JSON: &str = "application/json";

pub const ENCODING_SOLIDITY_ABI: &str = "application/x-solidity-abi";

const MAX_PACKET_BYTES: usize = 4096;

pub fn decode_packet(env: &Env, value: &Bytes) -> FungibleTokenPacketData {
    let len = value.len() as usize;
    if len == 0 || len > MAX_PACKET_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_PACKET_BYTES];
    value.copy_into_slice(&mut buf[..len]);
    let src = &buf[..len];

    let mut denom = String::from_str(env, "");
    let mut sender = String::from_str(env, "");
    let mut receiver = String::from_str(env, "");
    let mut memo = String::from_str(env, "");
    let mut amount: i128 = 0;

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

        let mut key = [0u8; 32];
        let (key_len, next) = parse_json_string(env, src, i, &mut key);
        i = skip_ws(src, next);
        if i >= src.len() || src[i] != b':' {
            panic_with_error!(env, Error::InvalidPacketData);
        }
        i = skip_ws(src, i + 1);

        let mut val = [0u8; MAX_PACKET_BYTES];
        let (val_len, next) = parse_json_string(env, src, i, &mut val);
        i = next;

        let k = &key[..key_len];
        let v = &val[..val_len];
        if k == b"denom" {
            denom = String::from_bytes(env, v);
        } else if k == b"amount" {
            amount = parse_i128(env, v);
        } else if k == b"sender" {
            sender = String::from_bytes(env, v);
        } else if k == b"receiver" {
            receiver = String::from_bytes(env, v);
        } else if k == b"memo" {
            memo = String::from_bytes(env, v);
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

    FungibleTokenPacketData {
        token: Token { denom, amount },
        sender,
        receiver,
        memo,
    }
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

fn parse_json_string(env: &Env, src: &[u8], start: usize, out: &mut [u8]) -> (usize, usize) {
    let mut i = start;
    if i >= src.len() || src[i] != b'"' {
        panic_with_error!(env, Error::InvalidPacketData);
    }
    i += 1;

    let mut o = 0usize;
    while i < src.len() {
        let c = src[i];
        if c == b'"' {
            return (o, i + 1);
        }

        if c == b'\\' {
            i += 1;
            if i >= src.len() {
                break;
            }
            match src[i] {
                b'"' => push_byte(env, out, &mut o, b'"'),
                b'\\' => push_byte(env, out, &mut o, b'\\'),
                b'/' => push_byte(env, out, &mut o, b'/'),
                b'b' => push_byte(env, out, &mut o, 0x08),
                b'f' => push_byte(env, out, &mut o, 0x0c),
                b'n' => push_byte(env, out, &mut o, b'\n'),
                b'r' => push_byte(env, out, &mut o, b'\r'),
                b't' => push_byte(env, out, &mut o, b'\t'),
                b'u' => {
                    if i + 4 >= src.len() {
                        panic_with_error!(env, Error::InvalidPacketData);
                    }
                    let cp = ((hex_to_nibble(env, src[i + 1]) as u32) << 12)
                        | ((hex_to_nibble(env, src[i + 2]) as u32) << 8)
                        | ((hex_to_nibble(env, src[i + 3]) as u32) << 4)
                        | (hex_to_nibble(env, src[i + 4]) as u32);
                    i += 4;
                    push_utf8(env, out, &mut o, cp);
                }
                _ => panic_with_error!(env, Error::InvalidPacketData),
            }
            i += 1;
        } else {
            push_byte(env, out, &mut o, c);
            i += 1;
        }
    }

    panic_with_error!(env, Error::InvalidPacketData);
}

fn push_byte(env: &Env, out: &mut [u8], o: &mut usize, b: u8) {
    if *o >= out.len() {
        panic_with_error!(env, Error::InvalidPacketData);
    }
    out[*o] = b;
    *o += 1;
}

fn push_utf8(env: &Env, out: &mut [u8], o: &mut usize, cp: u32) {
    if cp < 0x80 {
        push_byte(env, out, o, cp as u8);
    } else if cp < 0x800 {
        push_byte(env, out, o, 0xc0 | (cp >> 6) as u8);
        push_byte(env, out, o, 0x80 | (cp & 0x3f) as u8);
    } else {
        push_byte(env, out, o, 0xe0 | (cp >> 12) as u8);
        push_byte(env, out, o, 0x80 | ((cp >> 6) & 0x3f) as u8);
        push_byte(env, out, o, 0x80 | (cp & 0x3f) as u8);
    }
}

fn parse_i128(env: &Env, s: &[u8]) -> i128 {
    if s.is_empty() {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut n: i128 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            panic_with_error!(env, Error::InvalidPacketData);
        }
        n = n * 10 + (c - b'0') as i128;
    }

    n
}

pub fn encode_ics20_json(env: &Env, packet: &FungibleTokenPacketData) -> Bytes {
    let mut buf = Bytes::from_slice(env, b"{\"denom\":\"");
    append_string(env, &mut buf, &packet.token.denom);
    buf.append(&Bytes::from_slice(env, b"\",\"amount\":\""));
    append_decimal(env, &mut buf, packet.token.amount);
    buf.append(&Bytes::from_slice(env, b"\",\"sender\":\""));
    append_string(env, &mut buf, &packet.sender);
    buf.append(&Bytes::from_slice(env, b"\",\"receiver\":\""));
    append_string(env, &mut buf, &packet.receiver);
    buf.append(&Bytes::from_slice(env, b"\",\"memo\":\""));
    append_string(env, &mut buf, &packet.memo);
    buf.append(&Bytes::from_slice(env, b"\"}"));
    buf
}

const MAX_FIELD_BYTES: usize = 512;

fn append_string(env: &Env, buf: &mut Bytes, s: &String) {
    let len = s.len() as usize;
    if len > MAX_FIELD_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }
    let mut tmp = [0u8; MAX_FIELD_BYTES];
    s.copy_into_slice(&mut tmp[..len]);

    for &byte in &tmp[..len] {
        match byte {
            b'"' => buf.append(&Bytes::from_slice(env, b"\\\"")),
            b'\\' => buf.append(&Bytes::from_slice(env, b"\\\\")),
            0x08 => buf.append(&Bytes::from_slice(env, b"\\b")),
            0x0c => buf.append(&Bytes::from_slice(env, b"\\f")),
            b'\n' => buf.append(&Bytes::from_slice(env, b"\\n")),
            b'\r' => buf.append(&Bytes::from_slice(env, b"\\r")),
            b'\t' => buf.append(&Bytes::from_slice(env, b"\\t")),
            0x00..=0x1f => {
                buf.append(&Bytes::from_slice(env, b"\\u00"));
                buf.append(&Bytes::from_slice(
                    env,
                    &[hex_nibble(byte >> 4), hex_nibble(byte & 0x0f)],
                ));
            }
            _ => buf.append(&Bytes::from_slice(env, &[byte])),
        }
    }
}

fn hex_nibble(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'a' + (n - 10)
    }
}

fn append_decimal(env: &Env, buf: &mut Bytes, mut n: i128) {
    if n <= 0 {
        buf.append(&Bytes::from_slice(env, b"0"));

        return;
    }

    let mut digits = [0u8; 40];
    let mut i = 40usize;
    while n > 0 {
        i -= 1;
        digits[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    buf.append(&Bytes::from_slice(env, &digits[i..]));
}

pub(crate) fn is_success_ack(ack: &Bytes) -> bool {
    if ack.len() == 1 && ack.get(0).unwrap_or(0) == SUCCESS_ACK_BYTE {
        return true;
    }

    let prefix: &[u8] = b"{\"result\"";
    if ack.len() < prefix.len() as u32 {
        return false;
    }
    for (i, b) in prefix.iter().enumerate() {
        if ack.get(i as u32).unwrap_or(0) != *b {
            return false;
        }
    }

    true
}

pub fn address_to_string(_env: &Env, addr: &Address) -> String {
    addr.to_string()
}

pub(crate) fn string_to_address(_env: &Env, s: &String) -> Address {
    Address::from_string(s)
}

fn hex_to_nibble(env: &Env, c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic_with_error!(env, Error::InvalidPacketData),
    }
}

const ABI_WORD: usize = 32;

const ABI_HEAD_WORDS: usize = 5;

pub fn decode_payload(env: &Env, encoding: &String, value: &Bytes) -> FungibleTokenPacketData {
    if *encoding == String::from_str(env, ENCODING_JSON) {
        return decode_packet(env, value);
    }

    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return decode_packet_abi(env, value);
    }

    panic_with_error!(env, Error::UnsupportedEncoding);
}

pub fn encode_payload(env: &Env, encoding: &String, packet: &FungibleTokenPacketData) -> Bytes {
    if *encoding == String::from_str(env, ENCODING_JSON) {
        return encode_ics20_json(env, packet);
    }

    if *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI) {
        return encode_ics20_abi(env, packet);
    }

    panic_with_error!(env, Error::UnsupportedEncoding);
}

pub(crate) fn require_supported_encoding(env: &Env, encoding: &String) {
    let supported = *encoding == String::from_str(env, ENCODING_JSON)
        || *encoding == String::from_str(env, ENCODING_SOLIDITY_ABI);

    if !supported {
        panic_with_error!(env, Error::UnsupportedEncoding);
    }
}

pub fn encode_ics20_abi(env: &Env, packet: &FungibleTokenPacketData) -> Bytes {
    if packet.token.amount <= 0 {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let strings = [
        &packet.token.denom,
        &packet.sender,
        &packet.receiver,
        &packet.memo,
    ];

    let mut offsets = [0usize; 4];
    let mut tails = Bytes::new(env);

    for (index, field) in strings.iter().enumerate() {
        offsets[index] = ABI_HEAD_WORDS * ABI_WORD + tails.len() as usize;

        append_abi_string(env, &mut tails, field);
    }

    let mut out = Bytes::new(env);

    out.append(&abi_word(env, ABI_WORD as u128));
    out.append(&abi_word(env, offsets[0] as u128));
    out.append(&abi_word(env, offsets[1] as u128));
    out.append(&abi_word(env, offsets[2] as u128));
    out.append(&abi_word(env, packet.token.amount as u128));
    out.append(&abi_word(env, offsets[3] as u128));
    out.append(&tails);

    out
}

pub fn decode_packet_abi(env: &Env, value: &Bytes) -> FungibleTokenPacketData {
    let len = value.len() as usize;

    if len < (1 + ABI_HEAD_WORDS) * ABI_WORD || len > MAX_PACKET_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_PACKET_BYTES];
    value.copy_into_slice(&mut buf[..len]);
    let src = &buf[..len];

    if read_abi_offset(env, src, 0) != ABI_WORD {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let tuple = &src[ABI_WORD..];

    let denom = read_abi_string(env, tuple, read_abi_offset(env, tuple, 0));
    let sender = read_abi_string(env, tuple, read_abi_offset(env, tuple, ABI_WORD));
    let receiver = read_abi_string(env, tuple, read_abi_offset(env, tuple, 2 * ABI_WORD));
    let amount = read_abi_amount(env, &tuple[3 * ABI_WORD..4 * ABI_WORD]);
    let memo = read_abi_string(env, tuple, read_abi_offset(env, tuple, 4 * ABI_WORD));

    FungibleTokenPacketData {
        token: Token { denom, amount },
        sender,
        receiver,
        memo,
    }
}

fn abi_word(env: &Env, value: u128) -> Bytes {
    let mut word = [0u8; ABI_WORD];
    word[16..].copy_from_slice(&value.to_be_bytes());

    Bytes::from_slice(env, &word)
}

fn append_abi_string(env: &Env, buf: &mut Bytes, s: &String) {
    let len = s.len() as usize;

    if len > MAX_FIELD_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut tmp = [0u8; MAX_FIELD_BYTES];
    s.copy_into_slice(&mut tmp[..len]);

    let padding = (ABI_WORD - len % ABI_WORD) % ABI_WORD;

    buf.append(&abi_word(env, len as u128));
    buf.append(&Bytes::from_slice(env, &tmp[..len]));
    buf.append(&Bytes::from_slice(env, &[0u8; ABI_WORD][..padding]));
}

fn read_abi_offset(env: &Env, src: &[u8], at: usize) -> usize {
    if at + ABI_WORD > src.len() {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let word = &src[at..at + ABI_WORD];

    if word[..28].iter().any(|byte| *byte != 0) {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    u32::from_be_bytes([word[28], word[29], word[30], word[31]]) as usize
}

fn read_abi_string(env: &Env, tuple: &[u8], offset: usize) -> String {
    let len = read_abi_offset(env, tuple, offset);
    let start = offset + ABI_WORD;

    if len > MAX_FIELD_BYTES || start + len > tuple.len() {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    String::from_bytes(env, &tuple[start..start + len])
}

fn read_abi_amount(env: &Env, word: &[u8]) -> i128 {
    if word[..16].iter().any(|byte| *byte != 0) || word[16] & 0x80 != 0 {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut low = [0u8; 16];
    low.copy_from_slice(&word[16..]);

    let amount = u128::from_be_bytes(low) as i128;

    if amount <= 0 {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    amount
}

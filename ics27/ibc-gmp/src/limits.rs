use soroban_sdk::{panic_with_error, Env};

use crate::{errors::Error, types::GmpPacketData};

pub const MAX_SENDER_BYTES: u32 = 256;
pub const MAX_RECEIVER_BYTES: u32 = 256;
pub const MAX_SALT_BYTES: u32 = 64;
pub const MAX_PAYLOAD_BYTES: u32 = 32_768;
pub const MAX_MEMO_BYTES: u32 = 4_096;

pub fn check(env: &Env, packet: &GmpPacketData) {
    if packet.sender.len() > MAX_SENDER_BYTES {
        panic_with_error!(env, Error::SenderTooLong);
    }

    if packet.receiver.len() > MAX_RECEIVER_BYTES {
        panic_with_error!(env, Error::ReceiverTooLong);
    }

    if packet.salt.len() > MAX_SALT_BYTES {
        panic_with_error!(env, Error::SaltTooLong);
    }

    if packet.payload.len() > MAX_PAYLOAD_BYTES {
        panic_with_error!(env, Error::PayloadTooLong);
    }

    if packet.memo.len() > MAX_MEMO_BYTES {
        panic_with_error!(env, Error::MemoTooLong);
    }
}

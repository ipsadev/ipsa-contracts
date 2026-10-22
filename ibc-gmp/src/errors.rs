use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    RouterNotSet = 1,
    AccountWasmNotSet = 2,
    UnknownSourcePort = 3,
    UnknownDestPort = 4,
    UnsupportedVersion = 5,
    UnsupportedEncoding = 6,
    InvalidPacketData = 7,
    InvalidCallPayload = 8,
    SenderTooLong = 9,
    ReceiverTooLong = 10,
    SaltTooLong = 11,
    PayloadTooLong = 12,
    MemoTooLong = 13,
    TargetIsRouter = 14,
    ReceiverMismatch = 15,
}

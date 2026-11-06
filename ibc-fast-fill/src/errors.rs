use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotConfigured = 1,
    Unauthorized = 2,
    InvalidTerms = 3,
    InvalidAmount = 4,
    InvalidFee = 5,
    FillDeadlinePassed = 6,
    UnknownRoute = 7,
    AlreadyFilled = 8,
    AlreadySettled = 9,
    NotFilled = 10,
    AlreadyNotified = 11,
    NotAddressedHere = 12,
    InvalidEvmAddress = 13,
}

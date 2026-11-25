use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The router address has not been set in instance storage; the contract
    /// was not constructed or the constructor did not run.
    RouterNotSet = 1,
    /// The ICS-20 packet payload could not be decoded into a valid transfer.
    InvalidPacketData = 2,
    /// The account does not hold enough of the asset to cover the transfer.
    InsufficientBalance = 3,
    /// A transfer or mint amount was zero or negative; amounts must be positive.
    AmountMustBePositive = 4,
    /// The packet's source port does not match this application's transfer port.
    UnknownSourcePort = 5,
    /// The packet's destination port does not match this application's transfer port.
    UnknownDestPort = 6,
    /// The transfer would push the denom's usage past its configured daily cap.
    DailyLimitExceeded = 7,
    /// No asset is registered for the given denom.
    UnknownAsset = 8,
    /// The voucher-token wasm hash has not been set, so a voucher for an
    /// unregistered (foreign) denom cannot be deployed.
    VoucherWasmNotSet = 9,
    TooManyAssets = 10,
    UnsupportedEncoding = 11,
    InvalidCallback = 12,
    UnsupportedVersion = 13,
}

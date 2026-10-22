use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// No light client is registered for the given client type.
    UnknownClientType = 2,
    /// No client exists for the given client id.
    ClientIdNotFound = 3,
    /// A counterparty is already registered for this client.
    CounterpartyAlreadyRegistered = 4,
    /// The light client is frozen (misbehaviour detected) and cannot process packets.
    ClientFrozen = 5,
    /// An application is already registered for this port.
    PortAlreadyRegistered = 6,
    /// An IBC identifier is shorter or longer than the allowed bounds.
    InvalidIdentifierLength = 7,
    /// An IBC identifier does not start with the expected prefix.
    InvalidIdentifierPrefix = 8,
    /// An IBC identifier contains a disallowed character.
    InvalidIdentifierChar = 9,
    /// No counterparty is registered for the client; register it before relaying.
    CounterpartyNotRegistered = 10,
    /// The packet's timeout timestamp is already in the past at send time.
    TimeoutAlreadyElapsed = 11,
    /// No application is registered for the packet's port.
    PortNotRegistered = 12,
    /// The packet carries no payloads.
    PayloadsEmpty = 13,
    /// A receipt for this packet already exists (replay of an already-received packet).
    ReceiptAlreadyExists = 14,
    /// The packet's client/counterparty does not match the registered mapping.
    PacketCounterpartyMismatch = 15,
    /// The light client could not verify the membership proof for the commitment.
    MembershipVerificationFailed = 16,
    /// The light client could not verify the non-membership (absence) proof.
    NonMembershipVerificationFailed = 17,
    /// No stored packet commitment exists for the given sequence.
    NoCommitmentForSequence = 18,
    /// The stored commitment does not match the provided packet.
    CommitmentMismatch = 19,
    /// An acknowledgement for this packet has already been written.
    AckAlreadyExists = 20,
    /// No receipt is stored for the sequence; the ack/timeout path expects one.
    NoReceiptForSequence = 21,
    /// A timeout was requested but the packet's timeout has not yet elapsed.
    TimeoutNotYetElapsed = 22,
    ProvableStoreFull = 23,
    LightClientNotInitialised = 24,
    ProvableSlotTaken = 25,
    ProvableStoreCorrupt = 26,
    MultiplePayloadsNotSupported = 27,
    PortHasPacketsInFlight = 28,
}

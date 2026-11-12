#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error,
    xdr::{FromXdr, ToXdr},
    Address, Bytes, BytesN, Env, Map, String, Symbol, Val,
};
use stellar_access::access_control::set_admin;
use stellar_macros::only_admin;

mod ics23;
mod tmproto;
pub mod verify;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    ClientNotInitialised = 1,
    ClientFrozen = 2,
    InvalidClientState = 3,
    InvalidConsensusState = 4,
    InvalidHeader = 5,
    ConsensusStateMissing = 6,
    HeightAlreadyExistsWithDifferentRoot = 7,
    ChainIdMismatch = 8,
    HeaderHeightNotAfterTrusted = 9,
    TrustingPeriodElapsed = 10,
    ClientAlreadyInitialised = 11,
    HeaderTimeNotAfterTrusted = 12,
    HeaderTimeInFuture = 13,
    RevisionMismatch = 14,
    InvalidTrustLevel = 15,
    ValidatorSetHashMismatch = 16,
    CommitHeightMismatch = 17,
    BlockHashMismatch = 18,
    BadCommitSignature = 19,
    InsufficientVotingPower = 20,
    TrustLevelNotMet = 21,
    TargetHeightMismatch = 22,
}

fn header_error(reject: verify::Reject) -> Error {
    match reject {
        verify::Reject::Malformed => Error::InvalidHeader,
        verify::Reject::ChainIdMismatch => Error::ChainIdMismatch,
        verify::Reject::HeightNotAfterTrusted => Error::HeaderHeightNotAfterTrusted,
        verify::Reject::ValidatorSetHashMismatch => Error::ValidatorSetHashMismatch,
        verify::Reject::CommitHeightMismatch => Error::CommitHeightMismatch,
        verify::Reject::BlockHashMismatch => Error::BlockHashMismatch,
        verify::Reject::BadCommitSignature => Error::BadCommitSignature,
        verify::Reject::InsufficientVotingPower => Error::InsufficientVotingPower,
        verify::Reject::TrustLevelNotMet => Error::TrustLevelNotMet,
    }
}

#[contracttype]
#[derive(Clone)]
pub struct TrustThreshold {
    pub numerator: u32,
    pub denominator: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct Height {
    pub revision_number: u64,
    pub revision_height: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct TendermintClientState {
    pub chain_id: String,
    pub trust_level: TrustThreshold,
    pub trusting_period_secs: u64,
    pub unbonding_period_secs: u64,
    pub max_clock_drift_secs: u64,
    pub latest_height: Height,
    pub is_frozen: bool,
    pub frozen_height: Height,
    pub proof_specs: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct TendermintConsensusState {
    pub timestamp_secs: u64,
    pub next_validators_hash: BytesN<32>,
    pub root: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
pub struct TendermintHeader {
    pub trusted_height: Height,
    pub target_height: Height,
    pub timestamp_secs: u64,
    pub next_validators_hash: BytesN<32>,
    pub app_hash: BytesN<32>,
    pub signed_header_bytes: Bytes,
    pub validator_set_bytes: Bytes,
    pub trusted_validator_set_bytes: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct LegacyTendermintHeader {
    pub trusted_height: Height,
    pub target_height: Height,
    pub timestamp_secs: u64,
    pub next_validators_hash: BytesN<32>,
    pub app_hash: BytesN<32>,
    pub signed_header_bytes: Bytes,
    pub validator_set_bytes: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct Misbehaviour {
    pub header_a: TendermintHeader,
    pub header_b: TendermintHeader,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Client(String),
    Consensus(String, u64),
    Frozen(String),
}

#[contract]
pub struct TendermintLightClient;

#[contractimpl]
impl TendermintLightClient {
    pub fn __constructor(env: Env, router: Address) {
        set_admin(&env, &router);
    }

    #[only_admin]
    pub fn initialise(
        env: Env,
        client_id: String,
        client_state: Bytes,
        consensus_state: Bytes,
        height: u64,
    ) {
        if env
            .storage()
            .persistent()
            .has(&DataKey::Client(client_id.clone()))
        {
            panic_with_error!(&env, Error::ClientAlreadyInitialised);
        }

        let cs = TendermintClientState::from_xdr(&env, &client_state)
            .unwrap_or_else(|_| panic_with_error!(&env, Error::InvalidClientState));
        let cons = TendermintConsensusState::from_xdr(&env, &consensus_state)
            .unwrap_or_else(|_| panic_with_error!(&env, Error::InvalidConsensusState));

        let trust = &cs.trust_level;
        if trust.denominator == 0
            || (trust.numerator as u64) * 3 < trust.denominator as u64
            || trust.numerator > trust.denominator
        {
            panic_with_error!(&env, Error::InvalidTrustLevel);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Client(client_id.clone()), &cs);
        env.storage()
            .persistent()
            .set(&DataKey::Consensus(client_id, height), &cons);
    }

    pub fn latest_height(env: Env, client_id: String) -> u64 {
        load_client_state(&env, &client_id)
            .latest_height
            .revision_height
    }

    pub fn client_state(env: Env, client_id: String) -> Bytes {
        load_client_state(&env, &client_id).to_xdr(&env)
    }

    pub fn consensus_state(env: Env, client_id: String, height: u64) -> Bytes {
        load_consensus_state(&env, &client_id, height).to_xdr(&env)
    }

    pub fn verify_client_message(
        env: Env,
        client_id: String,
        client_message: Bytes,
    ) -> (Bytes, u64) {
        let check = verify_and_extract(&env, &client_id, &client_message);

        (Bytes::from_slice(&env, &check.app_hash), check.height)
    }

    pub fn check_for_misbehaviour(env: Env, client_id: String, client_message: Bytes) -> bool {
        if load_client_state(&env, &client_id).is_frozen {
            return false;
        }
        let check = verify_and_extract(&env, &client_id, &client_message);
        let app_hash = BytesN::from_array(&env, &check.app_hash);

        match env
            .storage()
            .persistent()
            .get::<DataKey, TendermintConsensusState>(&DataKey::Consensus(client_id, check.height))
        {
            Some(existing) => existing.root != app_hash,
            None => false,
        }
    }

    #[only_admin]
    pub fn update_state(env: Env, client_id: String, client_message: Bytes) -> u64 {
        let mut cs = load_client_state(&env, &client_id);
        if cs.is_frozen {
            panic_with_error!(&env, Error::ClientFrozen);
        }

        let header = decode_header(&env, &client_message);
        let check = verify::header_fields(&env, &header.signed_header_bytes)
            .unwrap_or_else(|| panic_with_error!(&env, Error::InvalidHeader));

        let new_consensus = TendermintConsensusState {
            timestamp_secs: check.timestamp,
            next_validators_hash: BytesN::from_array(&env, &check.next_validators_hash),
            root: BytesN::from_array(&env, &check.app_hash),
        };
        env.storage().persistent().set(
            &DataKey::Consensus(client_id.clone(), check.height),
            &new_consensus,
        );

        if check.height > cs.latest_height.revision_height {
            cs.latest_height = Height {
                revision_number: cs.latest_height.revision_number,
                revision_height: check.height,
            };
            env.storage()
                .persistent()
                .set(&DataKey::Client(client_id), &cs);
        }

        check.height
    }

    #[only_admin]
    pub fn update_state_on_misbehaviour(env: Env, client_id: String, client_message: Bytes) {
        let mut cs = load_client_state(&env, &client_id);
        cs.is_frozen = true;
        cs.frozen_height = cs.latest_height.clone();
        let _ = client_message;
        env.storage()
            .persistent()
            .set(&DataKey::Client(client_id.clone()), &cs);
        env.storage()
            .persistent()
            .set(&DataKey::Frozen(client_id), &true);
    }

    pub fn frozen(env: Env, client_id: String) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Frozen(client_id))
            .unwrap_or(false)
    }

    pub fn verify_membership(
        env: Env,
        client_id: String,
        height: u64,
        proof: Bytes,
        path: Bytes,
        value: Bytes,
    ) -> bool {
        let consensus = match env
            .storage()
            .persistent()
            .get::<DataKey, TendermintConsensusState>(&DataKey::Consensus(
                client_id.clone(),
                height,
            )) {
            Some(c) => c,
            None => return false,
        };
        if load_client_state(&env, &client_id).is_frozen {
            return false;
        }

        let root = consensus.root.to_array();

        ics23::verify_membership(&env, &root, &proof, &path, &value)
    }

    pub fn verify_non_membership(
        env: Env,
        client_id: String,
        height: u64,
        proof: Bytes,
        path: Bytes,
    ) -> bool {
        let consensus = match env
            .storage()
            .persistent()
            .get::<DataKey, TendermintConsensusState>(&DataKey::Consensus(
                client_id.clone(),
                height,
            )) {
            Some(c) => c,
            None => return false,
        };
        if load_client_state(&env, &client_id).is_frozen {
            return false;
        }

        let root = consensus.root.to_array();

        ics23::verify_non_membership(&env, &root, &proof, &path)
    }

    pub fn get_timestamp_at_height(env: Env, client_id: String, height: u64) -> u64 {
        env.storage()
            .persistent()
            .get::<DataKey, TendermintConsensusState>(&DataKey::Consensus(client_id, height))
            .map(|c| c.timestamp_secs)
            .unwrap_or(0)
    }
}

fn verify_and_extract(
    env: &Env,
    client_id: &String,
    client_message: &Bytes,
) -> verify::HeaderCheck {
    let header = decode_header(env, client_message);
    let cs = load_client_state(env, client_id);
    if cs.is_frozen {
        panic_with_error!(env, Error::ClientFrozen);
    }

    let trusted = env
        .storage()
        .persistent()
        .get::<DataKey, TendermintConsensusState>(&DataKey::Consensus(
            client_id.clone(),
            header.trusted_height.revision_height,
        ))
        .unwrap_or_else(|| panic_with_error!(env, Error::ConsensusStateMissing));

    let chain_id = string_to_bytes(env, &cs.chain_id);
    let trusted_next = trusted.next_validators_hash.to_array();

    let check = verify::verify_header(
        env,
        &header.signed_header_bytes,
        &header.validator_set_bytes,
        &chain_id,
        &verify::Trust {
            height: header.trusted_height.revision_height,
            next_validators_hash: &trusted_next,
            validator_set: &header.trusted_validator_set_bytes,
            numerator: cs.trust_level.numerator as u64,
            denominator: cs.trust_level.denominator as u64,
        },
    )
    .unwrap_or_else(|reject| panic_with_error!(env, header_error(reject)));

    if header.target_height.revision_number != header.trusted_height.revision_number {
        panic_with_error!(env, Error::RevisionMismatch);
    }

    if header.target_height.revision_height != check.height {
        panic_with_error!(env, Error::TargetHeightMismatch);
    }

    let now = env.ledger().timestamp();
    if now
        >= trusted
            .timestamp_secs
            .saturating_add(cs.trusting_period_secs)
    {
        panic_with_error!(env, Error::TrustingPeriodElapsed);
    }

    if check.timestamp <= trusted.timestamp_secs {
        panic_with_error!(env, Error::HeaderTimeNotAfterTrusted);
    }

    if check.timestamp >= now.saturating_add(cs.max_clock_drift_secs) {
        panic_with_error!(env, Error::HeaderTimeInFuture);
    }

    check
}

fn string_to_bytes(env: &Env, s: &String) -> Bytes {
    let len = s.len() as usize;
    let mut buf = [0u8; 256];
    s.copy_into_slice(&mut buf[..len]);

    Bytes::from_slice(env, &buf[..len])
}

fn load_client_state(env: &Env, client_id: &String) -> TendermintClientState {
    env.storage()
        .persistent()
        .get(&DataKey::Client(client_id.clone()))
        .unwrap_or_else(|| panic_with_error!(env, Error::ClientNotInitialised))
}

fn load_consensus_state(env: &Env, client_id: &String, height: u64) -> TendermintConsensusState {
    env.storage()
        .persistent()
        .get(&DataKey::Consensus(client_id.clone(), height))
        .unwrap_or_else(|| panic_with_error!(env, Error::ConsensusStateMissing))
}

fn decode_header(env: &Env, bytes: &Bytes) -> TendermintHeader {
    let fields = Map::<Symbol, Val>::from_xdr(env, bytes)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidHeader));

    if fields.contains_key(Symbol::new(env, "trusted_validator_set_bytes")) {
        return TendermintHeader::from_xdr(env, bytes)
            .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidHeader));
    }

    let legacy = LegacyTendermintHeader::from_xdr(env, bytes)
        .unwrap_or_else(|_| panic_with_error!(env, Error::InvalidHeader));

    TendermintHeader {
        trusted_height: legacy.trusted_height,
        target_height: legacy.target_height,
        timestamp_secs: legacy.timestamp_secs,
        next_validators_hash: legacy.next_validators_hash,
        app_hash: legacy.app_hash,
        signed_header_bytes: legacy.signed_header_bytes,
        validator_set_bytes: legacy.validator_set_bytes,
        trusted_validator_set_bytes: Bytes::new(env),
    }
}

#[allow(dead_code)]
fn _proof_specs_carrier(cs: &TendermintClientState) -> &Bytes {
    &cs.proof_specs
}

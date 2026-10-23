use soroban_sdk::{Bytes, BytesN, Env, Map, Vec};

use crate::tmproto::{
    append_fixed64_le, append_len_delimited, append_varint, merkle_root, message_field,
    repeated_field, to_array, varint_field,
};

const SIGNED_MSG_TYPE_PRECOMMIT: u8 = 2;
const BLOCK_ID_FLAG_COMMIT: u64 = 2;

pub struct HeaderCheck {
    pub height: u64,
    pub timestamp: u64,
    pub app_hash: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

pub struct Trust<'a> {
    pub height: u64,
    pub next_validators_hash: &'a [u8; 32],
    pub validator_set: &'a Bytes,
    pub numerator: u64,
    pub denominator: u64,
}

struct Signers {
    keys: Map<Bytes, BytesN<32>>,
    signed_power: u128,
    total_power: u128,
}

pub fn verify_header(
    env: &Env,
    signed_header: &Bytes,
    validator_set: &Bytes,
    expected_chain_id: &Bytes,
    trust: &Trust,
) -> Option<HeaderCheck> {
    let inner = message_field(signed_header, 1)?;
    let commit = message_field(signed_header, 2)?;

    let chain_id = message_field(&inner, 2)?;
    if &chain_id != expected_chain_id {
        return None;
    }

    let height = varint_field(&inner, 3)?;
    if height <= trust.height {
        return None;
    }

    let validators_hash = to_array::<32>(&message_field(&inner, 8)?)?;
    if validator_set_hash(env, validator_set)? != validators_hash {
        return None;
    }

    let block_id = message_field(&commit, 3)?;
    let block_hash = to_array::<32>(&message_field(&block_id, 1)?)?;
    if header_hash(env, &inner) != block_hash {
        return None;
    }

    let signers = commit_signers(env, signed_header, validator_set)?;
    if !exceeds(signers.signed_power, signers.total_power, 2, 3) {
        return None;
    }

    let unchanged = &validators_hash == trust.next_validators_hash;
    let adjacent = height == trust.height + 1;

    if !unchanged && (adjacent || !trusted_validators_signed(env, &signers, trust)) {
        return None;
    }

    let app_hash = to_array::<32>(&message_field(&inner, 11).unwrap_or_else(|| Bytes::new(env)))
        .unwrap_or([0u8; 32]);
    let next_validators_hash = to_array::<32>(&message_field(&inner, 9)?)?;
    let time = message_field(&inner, 4)?;
    let timestamp = varint_field(&time, 1).unwrap_or(0);

    Some(HeaderCheck {
        height,
        timestamp,
        app_hash,
        next_validators_hash,
    })
}

fn trusted_validators_signed(env: &Env, signers: &Signers, trust: &Trust) -> bool {
    if trust.validator_set.is_empty() || trust.denominator == 0 {
        return false;
    }

    if validator_set_hash(env, trust.validator_set).as_ref() != Some(trust.next_validators_hash) {
        return false;
    }

    let validators = match repeated_field(env, trust.validator_set, 1) {
        Some(validators) => validators,
        None => return false,
    };

    let mut total: u128 = 0;
    let mut signed: u128 = 0;

    for validator in validators.iter() {
        let voting_power = varint_field(&validator, 3).unwrap_or(0) as u128;
        total += voting_power;

        let (Some(address), Some(key)) = (message_field(&validator, 1), ed25519_key(&validator))
        else {
            continue;
        };

        if signers.keys.get(address) == Some(key) {
            signed += voting_power;
        }
    }

    exceeds(signed, total, trust.numerator, trust.denominator)
}

fn exceeds(part: u128, total: u128, numerator: u64, denominator: u64) -> bool {
    total > 0 && part * (denominator as u128) > total * (numerator as u128)
}

fn ed25519_key(validator: &Bytes) -> Option<BytesN<32>> {
    let pub_key = message_field(validator, 2)?;
    let ed25519 = message_field(&pub_key, 1)?;

    to_array::<32>(&ed25519).map(|key| BytesN::from_array(ed25519.env(), &key))
}

pub fn verify_commit(
    env: &Env,
    signed_header: &Bytes,
    validator_set: &Bytes,
    num: u64,
    den: u64,
) -> bool {
    match commit_signers(env, signed_header, validator_set) {
        Some(signers) => exceeds(signers.signed_power, signers.total_power, num, den),
        None => false,
    }
}

fn commit_signers(env: &Env, signed_header: &Bytes, validator_set: &Bytes) -> Option<Signers> {
    let header = message_field(signed_header, 1)?;
    let commit = message_field(signed_header, 2)?;
    let chain_id = message_field(&header, 2)?;
    let block_id = message_field(&commit, 3)?;
    let height = varint_field(&commit, 1)?;
    let round = varint_field(&commit, 2).unwrap_or(0);

    let validators = repeated_field(env, validator_set, 1)?;
    let signatures = repeated_field(env, &commit, 4)?;
    if validators.len() != signatures.len() || validators.is_empty() {
        return None;
    }

    let mut keys = Map::new(env);
    let mut total_power: u128 = 0;
    let mut signed_power: u128 = 0;

    for i in 0..validators.len() {
        let validator = validators.get(i).unwrap();
        let voting_power = varint_field(&validator, 3).unwrap_or(0);
        total_power += voting_power as u128;

        let signature = signatures.get(i).unwrap();
        let flag = varint_field(&signature, 1).unwrap_or(0);
        if flag != BLOCK_ID_FLAG_COMMIT {
            continue;
        }

        let timestamp = message_field(&signature, 3).unwrap_or_else(|| Bytes::new(env));
        let sig: BytesN<64> =
            BytesN::from_array(env, &to_array::<64>(&message_field(&signature, 4)?)?);
        let pub_key = ed25519_key(&validator)?;
        let address = message_field(&validator, 1)?;

        let sign_bytes = canonical_vote(env, height, round, &block_id, &timestamp, &chain_id);
        env.crypto().ed25519_verify(&pub_key, &sign_bytes, &sig);

        keys.set(address, pub_key);
        signed_power += voting_power as u128;
    }

    Some(Signers {
        keys,
        signed_power,
        total_power,
    })
}

fn canonical_vote(
    env: &Env,
    height: u64,
    round: u64,
    block_id: &Bytes,
    timestamp: &Bytes,
    chain_id: &Bytes,
) -> Bytes {
    let mut cv = Bytes::new(env);
    cv.push_back(0x08);
    cv.push_back(SIGNED_MSG_TYPE_PRECOMMIT);
    if height != 0 {
        cv.push_back(0x11);
        append_fixed64_le(&mut cv, height);
    }
    if round != 0 {
        cv.push_back(0x19);
        append_fixed64_le(&mut cv, round);
    }
    append_len_delimited(&mut cv, 0x22, block_id);
    append_len_delimited(&mut cv, 0x2a, timestamp);
    append_len_delimited(&mut cv, 0x32, chain_id);

    let mut signed = Bytes::new(env);
    append_varint(&mut signed, cv.len() as u64);
    signed.append(&cv);

    signed
}

pub fn header_hash(env: &Env, header: &Bytes) -> [u8; 32] {
    let mut leaves: Vec<Bytes> = Vec::new(env);

    leaves.push_back(field_or_empty(env, header, 1));
    leaves.push_back(cdc_bytes(env, &field_or_empty(env, header, 2)));
    leaves.push_back(cdc_int64(env, varint_field(header, 3).unwrap_or(0)));
    leaves.push_back(field_or_empty(env, header, 4));
    leaves.push_back(last_block_id_leaf(env, header));
    for field in 6..=14 {
        leaves.push_back(cdc_bytes(env, &field_or_empty(env, header, field)));
    }

    merkle_root(env, &leaves)
}

fn field_or_empty(env: &Env, msg: &Bytes, target: u64) -> Bytes {
    message_field(msg, target).unwrap_or_else(|| Bytes::new(env))
}

fn last_block_id_leaf(env: &Env, header: &Bytes) -> Bytes {
    match message_field(header, 5) {
        Some(id) => id,
        None => Bytes::from_slice(env, &[0x12, 0x00]),
    }
}

fn cdc_bytes(env: &Env, value: &Bytes) -> Bytes {
    let mut out = Bytes::new(env);
    if !value.is_empty() {
        append_len_delimited(&mut out, 0x0a, value);
    }

    out
}

fn cdc_int64(env: &Env, value: u64) -> Bytes {
    let mut out = Bytes::new(env);
    if value != 0 {
        out.push_back(0x08);
        append_varint(&mut out, value);
    }

    out
}

pub fn validator_set_hash(env: &Env, validator_set: &Bytes) -> Option<[u8; 32]> {
    let validators = repeated_field(env, validator_set, 1)?;

    let mut leaves: Vec<Bytes> = Vec::new(env);
    for validator in validators.iter() {
        let pub_key = message_field(&validator, 2)?;
        let voting_power = varint_field(&validator, 3).unwrap_or(0);

        let mut simple = Bytes::new(env);
        append_len_delimited(&mut simple, 0x0a, &pub_key);
        simple.push_back(0x10);
        append_varint(&mut simple, voting_power);

        leaves.push_back(simple);
    }

    if leaves.is_empty() {
        return None;
    }

    Some(merkle_root(env, &leaves))
}

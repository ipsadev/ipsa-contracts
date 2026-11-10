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

struct Commit {
    chain_id: Bytes,
    block_id: Bytes,
    height: u64,
    round: u64,
    signatures: Vec<Bytes>,
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

    if varint_field(&commit, 1)? != height {
        return None;
    }

    let block_id = message_field(&commit, 3)?;
    let block_hash = to_array::<32>(&message_field(&block_id, 1)?)?;
    if header_hash(env, &inner) != block_hash {
        return None;
    }

    let mut verified = Map::new(env);
    if !verify_commit_light(env, signed_header, validator_set, 2, 3, &mut verified)? {
        return None;
    }

    let unchanged = &validators_hash == trust.next_validators_hash;
    let adjacent = height == trust.height + 1;

    if !unchanged
        && (adjacent
            || !trusted_validators_signed(env, signed_header, validator_set, trust, &verified))
    {
        return None;
    }

    header_fields(env, signed_header)
}

pub fn header_fields(env: &Env, signed_header: &Bytes) -> Option<HeaderCheck> {
    let inner = message_field(signed_header, 1)?;

    let height = varint_field(&inner, 3)?;
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

fn trusted_validators_signed(
    env: &Env,
    signed_header: &Bytes,
    validator_set: &Bytes,
    trust: &Trust,
    verified: &Map<u32, BytesN<32>>,
) -> bool {
    if trust.validator_set.is_empty() || trust.denominator == 0 {
        return false;
    }

    if validator_set_hash(env, trust.validator_set).as_ref() != Some(trust.next_validators_hash) {
        return false;
    }

    let (Some(commit), Some(validators), Some(trusted)) = (
        commit_of(signed_header),
        repeated_field(env, validator_set, 1),
        repeated_field(env, trust.validator_set, 1),
    ) else {
        return false;
    };

    if validators.len() != commit.signatures.len() {
        return false;
    }

    let mut trusted_by_address: Map<Bytes, (BytesN<32>, u64)> = Map::new(env);
    let mut total: u128 = 0;

    for validator in trusted.iter() {
        let voting_power = varint_field(&validator, 3).unwrap_or(0);
        total += voting_power as u128;

        if let (Some(address), Some(key)) = (message_field(&validator, 1), ed25519_key(&validator))
        {
            trusted_by_address.set(address, (key, voting_power));
        }
    }

    let mut seen: Map<Bytes, ()> = Map::new(env);
    let mut tallied: u128 = 0;

    for index in 0..commit.signatures.len() {
        let signature = commit.signatures.get(index).unwrap();
        if varint_field(&signature, 1).unwrap_or(0) != BLOCK_ID_FLAG_COMMIT {
            continue;
        }

        let Some(address) = message_field(&signature, 2) else {
            continue;
        };
        let Some((key, voting_power)) = trusted_by_address.get(address.clone()) else {
            continue;
        };

        if seen.contains_key(address.clone()) {
            return false;
        }
        seen.set(address, ());

        if verified.get(index) != Some(key.clone()) && !verify_vote(env, &commit, &signature, &key)
        {
            return false;
        }

        tallied += voting_power as u128;

        if exceeds(tallied, total, trust.numerator, trust.denominator) {
            return true;
        }
    }

    false
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
    let mut verified = Map::new(env);

    verify_commit_light(env, signed_header, validator_set, num, den, &mut verified).unwrap_or(false)
}

fn commit_of(signed_header: &Bytes) -> Option<Commit> {
    let header = message_field(signed_header, 1)?;
    let commit = message_field(signed_header, 2)?;

    Some(Commit {
        chain_id: message_field(&header, 2)?,
        block_id: message_field(&commit, 3)?,
        height: varint_field(&commit, 1)?,
        round: varint_field(&commit, 2).unwrap_or(0),
        signatures: repeated_field(signed_header.env(), &commit, 4)?,
    })
}

fn verify_vote(env: &Env, commit: &Commit, signature: &Bytes, key: &BytesN<32>) -> bool {
    let Some(raw) = message_field(signature, 4).and_then(|raw| to_array::<64>(&raw)) else {
        return false;
    };

    let timestamp = message_field(signature, 3).unwrap_or_else(|| Bytes::new(env));
    let sign_bytes = canonical_vote(
        env,
        commit.height,
        commit.round,
        &commit.block_id,
        &timestamp,
        &commit.chain_id,
    );

    env.crypto()
        .ed25519_verify(key, &sign_bytes, &BytesN::from_array(env, &raw));

    true
}

fn verify_commit_light(
    env: &Env,
    signed_header: &Bytes,
    validator_set: &Bytes,
    num: u64,
    den: u64,
    verified: &mut Map<u32, BytesN<32>>,
) -> Option<bool> {
    let commit = commit_of(signed_header)?;
    let validators = repeated_field(env, validator_set, 1)?;

    if validators.len() != commit.signatures.len() || validators.is_empty() {
        return None;
    }

    let total: u128 = validators
        .iter()
        .map(|validator| varint_field(&validator, 3).unwrap_or(0) as u128)
        .sum();

    let mut tallied: u128 = 0;

    for index in 0..validators.len() {
        let signature = commit.signatures.get(index).unwrap();
        if varint_field(&signature, 1).unwrap_or(0) != BLOCK_ID_FLAG_COMMIT {
            continue;
        }

        let validator = validators.get(index).unwrap();
        if message_field(&signature, 2)? != message_field(&validator, 1)? {
            return None;
        }

        let key = ed25519_key(&validator)?;

        if !verify_vote(env, &commit, &signature, &key) {
            return None;
        }

        verified.set(index, key);
        tallied += varint_field(&validator, 3).unwrap_or(0) as u128;

        if exceeds(tallied, total, num, den) {
            return Some(true);
        }
    }

    Some(false)
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

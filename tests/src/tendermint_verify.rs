use prost::Message;
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Bytes, BytesN, Env, String};
use stellar_tendermint_light_client::{
    verify::{validator_set_hash, verify_commit},
    Error, Height, LegacyTendermintHeader, TendermintClientState, TendermintConsensusState,
    TendermintHeader, TendermintLightClient, TendermintLightClientClient, TrustThreshold,
};
use tendermint_testgen::{validator::generate_validators, Generator, Validator as TgValidator};

fn build_set(specs: &[(&str, u64)]) -> tendermint::validator::Set {
    let tg: std::vec::Vec<TgValidator> = specs
        .iter()
        .map(|(name, power)| TgValidator::new(name).voting_power(*power))
        .collect();
    let infos = generate_validators(&tg).unwrap();

    tendermint::validator::Set::new(infos, None)
}

#[test]
fn validator_set_hash_matches_tendermint() {
    let env = Env::default();

    let set = build_set(&[("a", 10), ("b", 20), ("c", 30)]);
    let expected = set.hash();

    let raw = tendermint_proto::types::ValidatorSet::from(set);
    let bytes = raw.encode_to_vec();

    let got = validator_set_hash(&env, &Bytes::from_slice(&env, &bytes)).unwrap();

    assert_eq!(&got[..], expected.as_bytes());
}

#[test]
fn validator_set_hash_matches_for_single_validator() {
    let env = Env::default();

    let set = build_set(&[("solo", 100)]);
    let expected = set.hash();

    let raw = tendermint_proto::types::ValidatorSet::from(set);
    let bytes = raw.encode_to_vec();

    let got = validator_set_hash(&env, &Bytes::from_slice(&env, &bytes)).unwrap();

    assert_eq!(&got[..], expected.as_bytes());
}

#[test]
fn validator_set_hash_matches_for_seven_validators() {
    let env = Env::default();

    let set = build_set(&[
        ("v1", 1),
        ("v2", 2),
        ("v3", 3),
        ("v4", 4),
        ("v5", 5),
        ("v6", 6),
        ("v7", 7),
    ]);
    let expected = set.hash();

    let raw = tendermint_proto::types::ValidatorSet::from(set);
    let bytes = raw.encode_to_vec();

    let got = validator_set_hash(&env, &Bytes::from_slice(&env, &bytes)).unwrap();

    assert_eq!(&got[..], expected.as_bytes());
}

fn gen_light_block(height: u64) -> tendermint::block::signed_header::SignedHeader {
    let lb = tendermint_testgen::LightBlock::new_default(height);
    lb.generate().unwrap().signed_header
}

#[test]
fn header_hash_matches_tendermint() {
    let env = Env::default();
    let sh = gen_light_block(5);
    let expected = sh.header.hash();

    let raw = tendermint_proto::types::Header::from(sh.header.clone());
    let bytes = raw.encode_to_vec();

    let got = stellar_tendermint_light_client::verify::header_hash(
        &env,
        &Bytes::from_slice(&env, &bytes),
    );

    assert_eq!(&got[..], expected.as_bytes());
    assert_eq!(sh.commit.block_id.hash.as_bytes(), expected.as_bytes());
}

#[test]
fn header_hash_matches_with_last_block_id_present() {
    let env = Env::default();
    let mut sh = gen_light_block(6);
    let bid = tendermint::block::Id {
        hash: tendermint::Hash::Sha256([7u8; 32]),
        part_set_header: tendermint::block::parts::Header::new(
            1,
            tendermint::Hash::Sha256([8u8; 32]),
        )
        .unwrap(),
    };
    sh.header.last_block_id = Some(bid);
    let expected = sh.header.hash();

    let raw = tendermint_proto::types::Header::from(sh.header.clone());
    let bytes = raw.encode_to_vec();

    let got = stellar_tendermint_light_client::verify::header_hash(
        &env,
        &Bytes::from_slice(&env, &bytes),
    );

    assert_eq!(&got[..], expected.as_bytes());
}

fn gen_full(
    height: u64,
) -> (
    tendermint::block::signed_header::SignedHeader,
    tendermint::validator::Set,
) {
    let lb = tendermint_testgen::LightBlock::new_default(height)
        .generate()
        .unwrap();

    (lb.signed_header, lb.validators)
}

#[test]
fn commit_verifies_against_testgen() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);

    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh).encode_to_vec();
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();

    assert!(stellar_tendermint_light_client::verify::verify_commit(
        &env,
        &Bytes::from_slice(&env, &sh_bytes),
        &Bytes::from_slice(&env, &vs_bytes),
        2,
        3,
    ));
}

#[test]
#[should_panic]
fn commit_rejects_tampered_signature() {
    let env = Env::default();
    let (mut sh, vals) = gen_full(5);

    let mut sig = sh.commit.signatures[0].clone();
    if let tendermint::block::CommitSig::BlockIdFlagCommit {
        signature: Some(s), ..
    } = &sig
    {
        let mut raw = s.clone().into_bytes();
        raw[0] ^= 0xff;
        if let tendermint::block::CommitSig::BlockIdFlagCommit { signature, .. } = &mut sig {
            *signature = Some(tendermint::Signature::try_from(raw).unwrap());
        }
    }
    sh.commit.signatures[0] = sig;

    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh).encode_to_vec();
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();

    stellar_tendermint_light_client::verify::verify_commit(
        &env,
        &Bytes::from_slice(&env, &sh_bytes),
        &Bytes::from_slice(&env, &vs_bytes),
        2,
        3,
    );
}

fn hash32(h: &tendermint::Hash) -> [u8; 32] {
    h.as_bytes().try_into().unwrap()
}

fn setup_client(
    env: &Env,
    sh: &tendermint::block::signed_header::SignedHeader,
    trusted_height: u64,
    chain_id: &str,
    trusted_next: [u8; 32],
) -> (
    stellar_tendermint_light_client::TendermintLightClientClient<'static>,
    soroban_sdk::String,
) {
    let router = soroban_sdk::Address::generate(env);
    let addr = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(env, &addr);
    let client_id = String::from_str(env, "07-tendermint-0");

    let cs = TendermintClientState {
        chain_id: String::from_str(env, chain_id),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: 1_000_000_000,
        unbonding_period_secs: 2_000_000_000,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: trusted_height,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    };
    let cons = TendermintConsensusState {
        timestamp_secs: 1,
        next_validators_hash: BytesN::from_array(env, &trusted_next),
        root: BytesN::from_array(env, &[0u8; 32]),
    };

    let _ = sh;
    client.initialise(
        &client_id,
        &cs.to_xdr(env),
        &cons.to_xdr(env),
        &trusted_height,
    );

    (client, client_id)
}

fn build_update_msg(
    env: &Env,
    sh: &tendermint::block::signed_header::SignedHeader,
    vs_bytes: &[u8],
    trusted_height: u64,
) -> Bytes {
    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh.clone()).encode_to_vec();
    let header = TendermintHeader {
        trusted_height: Height {
            revision_number: 0,
            revision_height: trusted_height,
        },
        target_height: Height {
            revision_number: 0,
            revision_height: sh.header.height.value(),
        },
        timestamp_secs: 0,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        app_hash: BytesN::from_array(env, &[0u8; 32]),
        signed_header_bytes: Bytes::from_slice(env, &sh_bytes),
        validator_set_bytes: Bytes::from_slice(env, vs_bytes),
        trusted_validator_set_bytes: Bytes::new(env),
    };

    header.to_xdr(env)
}

#[test]
fn verify_client_message_accepts_valid_header() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh.header.validators_hash);

    let (client, cid) = setup_client(&env, &sh, 4, sh.header.chain_id.as_str(), trusted_next);
    let msg = build_update_msg(&env, &sh, &vs_bytes, 4);

    let (_root, height) = client.verify_client_message(&cid, &msg);
    assert_eq!(height, 5);

    assert_eq!(client.update_state(&cid, &msg), 5);
}

#[test]
#[should_panic]
fn verify_client_message_rejects_wrong_chain_id() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh.header.validators_hash);

    let (client, cid) = setup_client(&env, &sh, 4, "wrong-chain", trusted_next);
    let msg = build_update_msg(&env, &sh, &vs_bytes, 4);

    client.verify_client_message(&cid, &msg);
}

#[test]
#[should_panic]
fn verify_client_message_rejects_broken_adjacency() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();

    let (client, cid) = setup_client(&env, &sh, 4, sh.header.chain_id.as_str(), [0u8; 32]);
    let msg = build_update_msg(&env, &sh, &vs_bytes, 4);

    client.verify_client_message(&cid, &msg);
}

fn from_hex(env: &Env, s: &str) -> Bytes {
    let raw: std::vec::Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();

    Bytes::from_slice(env, &raw)
}

#[test]
fn verify_commit_accepts_real_round0_precommit() {
    let env = Env::default();

    let signed_header = from_hex(&env, "0a8e030a02080b120673696d642d3118c5b105220c08cae9e0d20610b7c0e998032a480a201a269a71a4413f30b77c0895b8be8b88f3f992300d852024ca3dd746b568afe1122408011220280d8aba9270e0b0c24d65c47f2b2b9926d42c4d4fafc78d950605b51b5003993220a2541d5d98c99b84b2719df8283335927e54f0f2f218a22f96fb247e55e4fbd73a20c35c4e5715ec03af22c491739a205c8bb139f0eaa716be2aa7e703ff0104b8e042208a4a2ad1ca2cfc6796a0d1e705500b0c9a0d43183372495942f83885c57efc624a208a4a2ad1ca2cfc6796a0d1e705500b0c9a0d43183372495942f83885c57efc625220048091bc7ddc283f77bfbf91d73c44da58c3df8a9cbc867405d8b7f3daada22f5a20c95b72540b3a2dccbbab1b278a8510c88b854d346aa246ba69fed71c60053faf6220e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8556a20e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85572145f8c49826543da4cdda21511843a790e27e6a4ff12b80108c5b1051a480a201b5d5042355ec94d735ee8abbb4b4a4c9c0cdd02c32b68223bcd375328588a3d1224080112201ff0ca2ec1459c2a44a7cbe658a0d6f46efaa660231eec916b9f39c3916d5ccf2268080212145f8c49826543da4cdda21511843a790e27e6a4ff1a0c08cfe9e0d20610928ca99f032240ce9a24135fa2561f035d68be276910517853c9e144cc5696dc43f508b866ab964fabd6028954e1e40b8c97fcadde32fba277f6f4a2d95908ff35474cb7bdc603");
    let validator_set = from_hex(&env, "0a3e0a145f8c49826543da4cdda21511843a790e27e6a4ff12220a206126d8cc7e37041e28ec09b72ae96c195043552f6cfb07904327b1c67302bccc18a08d06123e0a145f8c49826543da4cdda21511843a790e27e6a4ff12220a206126d8cc7e37041e28ec09b72ae96c195043552f6cfb07904327b1c67302bccc18a08d0618a08d06");

    assert!(verify_commit(&env, &signed_header, &validator_set, 2, 3));
}

fn forged_update_msg(
    env: &Env,
    sh: &tendermint::block::signed_header::SignedHeader,
    vs_bytes: &[u8],
    trusted_height: u64,
    app_hash: [u8; 32],
) -> Bytes {
    let mut sh = sh.clone();
    for sig in sh.commit.signatures.iter_mut() {
        if let tendermint::block::CommitSig::BlockIdFlagCommit {
            signature: Some(s), ..
        } = sig
        {
            let mut raw = s.clone().into_bytes();
            raw[0] ^= 0xff;
            *s = tendermint::Signature::try_from(raw).unwrap();
        }
    }

    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh.clone()).encode_to_vec();
    let header = TendermintHeader {
        trusted_height: Height {
            revision_number: 0,
            revision_height: trusted_height,
        },
        target_height: Height {
            revision_number: 0,
            revision_height: sh.header.height.value(),
        },
        timestamp_secs: 0,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        app_hash: BytesN::from_array(env, &app_hash),
        signed_header_bytes: Bytes::from_slice(env, &sh_bytes),
        validator_set_bytes: Bytes::from_slice(env, vs_bytes),
        trusted_validator_set_bytes: Bytes::new(env),
    };

    header.to_xdr(env)
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn misbehaviour_rejects_a_header_no_validator_signed() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh.header.validators_hash);

    let (client, cid) = setup_client(&env, &sh, 4, sh.header.chain_id.as_str(), trusted_next);
    client.update_state(&cid, &build_update_msg(&env, &sh, &vs_bytes, 4));

    let forged = forged_update_msg(&env, &sh, &vs_bytes, 4, [0xAA; 32]);
    client.check_for_misbehaviour(&cid, &forged);
}

#[test]
fn misbehaviour_reports_a_signed_header_that_contradicts_stored_state() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh.header.validators_hash);

    let (client, cid) = setup_client(&env, &sh, 4, sh.header.chain_id.as_str(), trusted_next);
    let honest = build_update_msg(&env, &sh, &vs_bytes, 4);
    client.update_state(&cid, &honest);

    assert!(
        !client.check_for_misbehaviour(&cid, &honest),
        "a header agreeing with stored state is not misbehaviour"
    );
}
fn with_one_absent_vote(
    sh: &tendermint::block::signed_header::SignedHeader,
) -> tendermint::block::signed_header::SignedHeader {
    let mut sh = sh.clone();
    sh.commit.signatures[1] = tendermint::block::CommitSig::BlockIdFlagAbsent;
    sh
}

#[test]
fn commit_skips_an_absent_vote_instead_of_trapping() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let sh = with_one_absent_vote(&sh);

    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh).encode_to_vec();
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();

    assert!(
        stellar_tendermint_light_client::verify::verify_commit(
            &env,
            &Bytes::from_slice(&env, &sh_bytes),
            &Bytes::from_slice(&env, &vs_bytes),
            1,
            3,
        ),
        "an absent vote must not abort verification of an otherwise valid commit"
    );
}

#[test]
fn an_absent_vote_contributes_no_voting_power() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let sh = with_one_absent_vote(&sh);

    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh).encode_to_vec();
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();

    assert!(
        !stellar_tendermint_light_client::verify::verify_commit(
            &env,
            &Bytes::from_slice(&env, &sh_bytes),
            &Bytes::from_slice(&env, &vs_bytes),
            2,
            3,
        ),
        "half the power cannot satisfy a two-thirds threshold"
    );
}

#[test]
fn a_historical_update_does_not_move_the_latest_height_backwards() {
    let env = Env::default();
    let (sh_new, vals) = gen_full(7);
    let (sh_old, _) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh_new.header.validators_hash);

    let (client, cid) = setup_client(
        &env,
        &sh_new,
        4,
        sh_new.header.chain_id.as_str(),
        trusted_next,
    );

    assert_eq!(
        client.update_state(&cid, &build_update_msg(&env, &sh_new, &vs_bytes, 4)),
        7
    );
    assert_eq!(client.latest_height(&cid), 7);

    assert_eq!(
        client.update_state(&cid, &build_update_msg(&env, &sh_old, &vs_bytes, 4)),
        5
    );
    assert_eq!(
        client.latest_height(&cid),
        7,
        "a replayed historical header must not lower the latest height"
    );
}

#[test]
fn a_historical_update_still_records_its_consensus_state() {
    let env = Env::default();
    let (sh_new, vals) = gen_full(7);
    let (sh_old, _) = gen_full(5);
    let vs_bytes = tendermint_proto::types::ValidatorSet::from(vals).encode_to_vec();
    let trusted_next = hash32(&sh_new.header.validators_hash);

    let (client, cid) = setup_client(
        &env,
        &sh_new,
        4,
        sh_new.header.chain_id.as_str(),
        trusted_next,
    );
    client.update_state(&cid, &build_update_msg(&env, &sh_new, &vs_bytes, 4));
    client.update_state(&cid, &build_update_msg(&env, &sh_old, &vs_bytes, 4));

    assert!(
        !client.consensus_state(&cid, &5).is_empty(),
        "backfilling must keep working — the older state is still stored"
    );
    assert!(!client.consensus_state(&cid, &7).is_empty());
}

fn validators(specs: &[(&str, u64)]) -> std::vec::Vec<TgValidator> {
    specs
        .iter()
        .map(|(name, power)| TgValidator::new(name).voting_power(*power))
        .collect()
}

fn signed_by(
    height: u64,
    signers: &[TgValidator],
) -> (
    tendermint::block::signed_header::SignedHeader,
    tendermint::validator::Set,
) {
    let header = tendermint_testgen::Header::new(signers)
        .height(height)
        .chain_id("test-chain")
        .next_validators(signers)
        .time(tendermint::Time::from_unix_timestamp(height as i64, 0).unwrap());
    let block = tendermint_testgen::LightBlock::new_default_with_header(header)
        .generate()
        .unwrap();

    (block.signed_header, block.validators)
}

fn set_bytes(set: tendermint::validator::Set) -> std::vec::Vec<u8> {
    tendermint_proto::types::ValidatorSet::from(set).encode_to_vec()
}

fn skipping_msg(
    env: &Env,
    sh: &tendermint::block::signed_header::SignedHeader,
    validator_set: &[u8],
    trusted_validator_set: &[u8],
    trusted_height: u64,
) -> Bytes {
    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh.clone()).encode_to_vec();

    TendermintHeader {
        trusted_height: Height {
            revision_number: 0,
            revision_height: trusted_height,
        },
        target_height: Height {
            revision_number: 0,
            revision_height: sh.header.height.value(),
        },
        timestamp_secs: 0,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        app_hash: BytesN::from_array(env, &[0u8; 32]),
        signed_header_bytes: Bytes::from_slice(env, &sh_bytes),
        validator_set_bytes: Bytes::from_slice(env, validator_set),
        trusted_validator_set_bytes: Bytes::from_slice(env, trusted_validator_set),
    }
    .to_xdr(env)
}

struct Changed {
    header: tendermint::block::signed_header::SignedHeader,
    validator_set: std::vec::Vec<u8>,
    trusted_set: std::vec::Vec<u8>,
    trusted_next: [u8; 32],
}

fn changed_set(trusted: &[(&str, u64)], current: &[(&str, u64)]) -> Changed {
    let trusted_set = build_set(trusted);
    let trusted_next = hash32(&trusted_set.hash());
    let (header, validator_set) = signed_by(10, &validators(current));

    Changed {
        header,
        validator_set: set_bytes(validator_set),
        trusted_set: set_bytes(trusted_set),
        trusted_next,
    }
}

#[test]
fn a_header_after_a_validator_set_change_is_accepted_when_the_trusted_validators_signed_it() {
    let env = Env::default();
    let changed = changed_set(
        &[("a", 10), ("b", 10), ("c", 10), ("d", 10)],
        &[("a", 10), ("b", 10), ("c", 10), ("e", 10)],
    );

    let (client, cid) = setup_client(
        &env,
        &changed.header,
        5,
        changed.header.header.chain_id.as_str(),
        changed.trusted_next,
    );
    let msg = skipping_msg(
        &env,
        &changed.header,
        &changed.validator_set,
        &changed.trusted_set,
        5,
    );

    assert_eq!(client.update_state(&cid, &msg), 10);
    assert_eq!(client.latest_height(&cid), 10);
}

#[test]
fn a_header_signed_by_too_few_trusted_validators_is_refused() {
    let env = Env::default();
    let changed = changed_set(
        &[("a", 10), ("b", 10), ("c", 10)],
        &[("a", 10), ("d", 10), ("e", 10)],
    );

    let (client, cid) = setup_client(
        &env,
        &changed.header,
        5,
        changed.header.header.chain_id.as_str(),
        changed.trusted_next,
    );
    let msg = skipping_msg(
        &env,
        &changed.header,
        &changed.validator_set,
        &changed.trusted_set,
        5,
    );

    assert_eq!(
        client.try_update_state(&cid, &msg),
        Err(Ok(Error::InvalidHeader.into()))
    );
}

#[test]
fn a_trusted_set_that_is_not_the_one_the_client_trusts_is_refused() {
    let env = Env::default();
    let changed = changed_set(
        &[("a", 10), ("b", 10), ("c", 10), ("d", 10)],
        &[("a", 10), ("b", 10), ("c", 10), ("e", 10)],
    );

    let (client, cid) = setup_client(
        &env,
        &changed.header,
        5,
        changed.header.header.chain_id.as_str(),
        changed.trusted_next,
    );
    let msg = skipping_msg(
        &env,
        &changed.header,
        &changed.validator_set,
        &changed.validator_set,
        5,
    );

    assert_eq!(
        client.try_update_state(&cid, &msg),
        Err(Ok(Error::InvalidHeader.into()))
    );
}

#[test]
fn a_changed_set_without_the_trusted_validators_is_refused() {
    let env = Env::default();
    let changed = changed_set(
        &[("a", 10), ("b", 10), ("c", 10), ("d", 10)],
        &[("a", 10), ("b", 10), ("c", 10), ("e", 10)],
    );

    let (client, cid) = setup_client(
        &env,
        &changed.header,
        5,
        changed.header.header.chain_id.as_str(),
        changed.trusted_next,
    );
    let msg = skipping_msg(&env, &changed.header, &changed.validator_set, &[], 5);

    assert_eq!(
        client.try_update_state(&cid, &msg),
        Err(Ok(Error::InvalidHeader.into()))
    );
}

#[test]
fn an_adjacent_header_must_carry_the_trusted_next_validators() {
    let env = Env::default();
    let changed = changed_set(
        &[("a", 10), ("b", 10), ("c", 10), ("d", 10)],
        &[("a", 10), ("b", 10), ("c", 10), ("e", 10)],
    );

    let (client, cid) = setup_client(
        &env,
        &changed.header,
        9,
        changed.header.header.chain_id.as_str(),
        changed.trusted_next,
    );
    let msg = skipping_msg(
        &env,
        &changed.header,
        &changed.validator_set,
        &changed.trusted_set,
        9,
    );

    assert_eq!(
        client.try_update_state(&cid, &msg),
        Err(Ok(Error::InvalidHeader.into()))
    );
}

#[test]
fn a_header_without_the_trusted_validators_field_still_updates_an_unchanged_set() {
    let env = Env::default();
    let (sh, vals) = gen_full(5);
    let vs_bytes = set_bytes(vals);
    let trusted_next = hash32(&sh.header.validators_hash);

    let (client, cid) = setup_client(&env, &sh, 4, sh.header.chain_id.as_str(), trusted_next);
    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh.clone()).encode_to_vec();
    let legacy = LegacyTendermintHeader {
        trusted_height: Height {
            revision_number: 0,
            revision_height: 4,
        },
        target_height: Height {
            revision_number: 0,
            revision_height: 5,
        },
        timestamp_secs: 0,
        next_validators_hash: BytesN::from_array(&env, &[0u8; 32]),
        app_hash: BytesN::from_array(&env, &[0u8; 32]),
        signed_header_bytes: Bytes::from_slice(&env, &sh_bytes),
        validator_set_bytes: Bytes::from_slice(&env, &vs_bytes),
    };

    assert_eq!(client.update_state(&cid, &legacy.to_xdr(&env)), 5);
}

const HUB_TRUSTED_HEIGHT: u64 = 19532136;

const HUB_TARGET_HEIGHT: u64 = 19534611;

const HUB_TRUSTED_NEXT_VALIDATORS: &str =
    "0ed48a2688ff5043e4057a43ed99dd21d753637f7a567e05801598ee44eefbab";

fn fixture(name: &str) -> std::vec::Vec<u8> {
    let path = format!(
        "{}/fixtures/hub-validator-set-change/{name}.hex",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap();

    (0..text.trim().len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn the_hub_header_after_its_validator_set_change_is_accepted_within_the_budget() {
    let env = Env::default();
    let signed_header = fixture("signed_header");
    let header = tendermint_proto::types::SignedHeader::decode(signed_header.as_slice()).unwrap();
    let header: tendermint::block::signed_header::SignedHeader = header.try_into().unwrap();

    let trusted_next: [u8; 32] = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&HUB_TRUSTED_NEXT_VALIDATORS[i..i + 2], 16).unwrap())
        .collect::<std::vec::Vec<u8>>()
        .try_into()
        .unwrap();

    let (client, cid) = setup_client(&env, &header, HUB_TRUSTED_HEIGHT, "provider", trusted_next);
    let msg = skipping_msg(
        &env,
        &header,
        &fixture("validator_set"),
        &fixture("trusted_validator_set"),
        HUB_TRUSTED_HEIGHT,
    );

    env.cost_estimate().budget().reset_default();

    assert_eq!(client.update_state(&cid, &msg), HUB_TARGET_HEIGHT);

    let budget = env.cost_estimate().budget();
    eprintln!(
        "hub update across a validator set change: {} cpu, {} mem",
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost()
    );
}

#[test]
fn the_hub_header_after_its_validator_set_change_is_refused_without_the_trusted_validators() {
    let env = Env::default();
    let signed_header = fixture("signed_header");
    let header = tendermint_proto::types::SignedHeader::decode(signed_header.as_slice()).unwrap();
    let header: tendermint::block::signed_header::SignedHeader = header.try_into().unwrap();

    let trusted_next: [u8; 32] = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&HUB_TRUSTED_NEXT_VALIDATORS[i..i + 2], 16).unwrap())
        .collect::<std::vec::Vec<u8>>()
        .try_into()
        .unwrap();

    let (client, cid) = setup_client(&env, &header, HUB_TRUSTED_HEIGHT, "provider", trusted_next);
    let msg = skipping_msg(
        &env,
        &header,
        &fixture("validator_set"),
        &[],
        HUB_TRUSTED_HEIGHT,
    );

    assert_eq!(
        client.try_update_state(&cid, &msg),
        Err(Ok(Error::InvalidHeader.into()))
    );
}

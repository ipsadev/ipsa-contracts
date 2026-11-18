use prost::Message;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, String,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient};
use stellar_tendermint_light_client::{
    Error, Height, Misbehaviour, TendermintClientState, TendermintConsensusState, TendermintHeader,
    TendermintLightClient, TendermintLightClientClient, TrustThreshold,
};
use tendermint_testgen::{Generator, Validator as TgValidator};

const CHAIN_ID: &str = "test-chain";
const CLIENT_TYPE: &str = "07-tendermint";
const TRUSTING_PERIOD_SECS: u64 = 10_000_000_000;
const UNBONDING_PERIOD_SECS: u64 = 20_000_000_000;

const TRUSTED_HEIGHT: u64 = 9;
const TRUSTED_TIME: u64 = 9;
const NOW: u64 = 1_000;

fn signers() -> std::vec::Vec<TgValidator> {
    ["a", "b", "c", "d"]
        .iter()
        .map(|name| TgValidator::new(name).voting_power(10))
        .collect()
}

fn signed_header(
    height: u64,
    time: u64,
    app_hash: &[u8],
) -> (
    tendermint::block::signed_header::SignedHeader,
    tendermint::validator::Set,
) {
    let set = signers();
    let header = tendermint_testgen::Header::new(&set)
        .height(height)
        .chain_id(CHAIN_ID)
        .next_validators(&set)
        .time(tendermint::Time::from_unix_timestamp(time as i64, 0).unwrap())
        .app_hash(tendermint::AppHash::try_from(app_hash.to_vec()).unwrap());
    let block = tendermint_testgen::LightBlock::new_default_with_header(header)
        .generate()
        .unwrap();

    (block.signed_header, block.validators)
}

fn set_bytes(set: &tendermint::validator::Set) -> std::vec::Vec<u8> {
    tendermint_proto::types::ValidatorSet::from(set.clone()).encode_to_vec()
}

fn hash32(hash: &tendermint::Hash) -> [u8; 32] {
    hash.as_bytes().try_into().unwrap()
}

fn tendermint_header(
    env: &Env,
    sh: &tendermint::block::signed_header::SignedHeader,
    validator_set: &tendermint::validator::Set,
) -> TendermintHeader {
    let sh_bytes = tendermint_proto::types::SignedHeader::from(sh.clone()).encode_to_vec();

    TendermintHeader {
        trusted_height: Height {
            revision_number: 0,
            revision_height: TRUSTED_HEIGHT,
        },
        target_height: Height {
            revision_number: 0,
            revision_height: sh.header.height.value(),
        },
        timestamp_secs: 0,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        app_hash: BytesN::from_array(env, &[0u8; 32]),
        signed_header_bytes: Bytes::from_slice(env, &sh_bytes),
        validator_set_bytes: Bytes::from_slice(env, &set_bytes(validator_set)),
        trusted_validator_set_bytes: Bytes::new(env),
    }
}

fn header_message(env: &Env, height: u64, time: u64, app_hash: &[u8]) -> Bytes {
    let (sh, set) = signed_header(height, time, app_hash);

    tendermint_header(env, &sh, &set).to_xdr(env)
}

fn misbehaviour_message(
    env: &Env,
    header_a: (u64, u64, &[u8]),
    header_b: (u64, u64, &[u8]),
) -> Bytes {
    let (sh_a, set_a) = signed_header(header_a.0, header_a.1, header_a.2);
    let (sh_b, set_b) = signed_header(header_b.0, header_b.1, header_b.2);

    Misbehaviour {
        header_a: tendermint_header(env, &sh_a, &set_a),
        header_b: tendermint_header(env, &sh_b, &set_b),
    }
    .to_xdr(env)
}

fn trusted_next() -> [u8; 32] {
    let (sh, _) = signed_header(TRUSTED_HEIGHT + 1, TRUSTED_HEIGHT + 1, &[0u8; 32]);

    hash32(&sh.header.validators_hash)
}

fn client_state(env: &Env) -> Bytes {
    TendermintClientState {
        chain_id: String::from_str(env, CHAIN_ID),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: TRUSTING_PERIOD_SECS,
        unbonding_period_secs: UNBONDING_PERIOD_SECS,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: TRUSTED_HEIGHT,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    }
    .to_xdr(env)
}

fn consensus_state(env: &Env) -> Bytes {
    TendermintConsensusState {
        timestamp_secs: TRUSTED_TIME,
        next_validators_hash: BytesN::from_array(env, &trusted_next()),
        root: BytesN::from_array(env, &[0u8; 32]),
    }
    .to_xdr(env)
}

struct Fixture {
    env: Env,
    client: TendermintLightClientClient<'static>,
    client_id: String,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.ledger().set_timestamp(NOW);

    let router = Address::generate(&env);
    let address = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(&env, &address);
    let client_id = String::from_str(&env, "07-tendermint-0");

    client.initialise(
        &client_id,
        &client_state(&env),
        &consensus_state(&env),
        &TRUSTED_HEIGHT,
    );

    Fixture {
        env,
        client,
        client_id,
    }
}

fn apply_update(fx: &Fixture, height: u64, time: u64) {
    let message = header_message(&fx.env, height, time, &[0u8; 32]);

    assert!(
        !fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a lone honest header is not misbehaviour"
    );
    fx.client.update_state(&fx.client_id, &message);
}

#[test]
fn two_conflicting_headers_at_one_height_freeze_the_client() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (10, 10, &[0xaa; 32]), (10, 10, &[0xbb; 32]));

    assert!(
        fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "two valid headers at one height with different block hashes are misbehaviour"
    );

    fx.client
        .update_state_on_misbehaviour(&fx.client_id, &message);

    assert!(
        fx.client.frozen(&fx.client_id),
        "detected misbehaviour freezes the client"
    );
}

#[test]
fn two_identical_headers_are_rejected_as_not_misbehaviour() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (10, 10, &[0xaa; 32]), (10, 10, &[0xaa; 32]));

    assert_eq!(
        fx.client
            .try_check_for_misbehaviour(&fx.client_id, &message),
        Err(Ok(Error::NotMisbehaviour.into())),
        "the same header submitted twice shares a block hash and is not a fork"
    );
    assert!(!fx.client.frozen(&fx.client_id));
}

#[test]
fn two_headers_whose_times_run_backwards_freeze_the_client() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (15, 10, &[0xaa; 32]), (12, 20, &[0xbb; 32]));

    assert!(
        fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "the higher header carrying the earlier time is a monotonic-time violation"
    );
}

#[test]
fn two_headers_in_increasing_time_order_are_rejected_as_not_misbehaviour() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (15, 20, &[0xaa; 32]), (12, 12, &[0xbb; 32]));

    assert_eq!(
        fx.client
            .try_check_for_misbehaviour(&fx.client_id, &message),
        Err(Ok(Error::NotMisbehaviour.into())),
        "a higher header with a later time respects monotonic time"
    );
    assert!(!fx.client.frozen(&fx.client_id));
}

#[test]
fn two_honest_headers_in_the_same_second_do_not_freeze_the_client() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (11, 50, &[0xaa; 32]), (10, 50, &[0xbb; 32]));

    assert_eq!(
        fx.client
            .try_check_for_misbehaviour(&fx.client_id, &message),
        Err(Ok(Error::NotMisbehaviour.into())),
        "consecutive honest headers sharing a second are not a time violation"
    );
    assert!(
        !fx.client.frozen(&fx.client_id),
        "same-second headers must not be a permissionless griefing freeze"
    );
}

#[test]
fn misbehaviour_evidence_timed_at_or_before_the_trusted_state_still_freezes() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (11, 3, &[0xaa; 32]), (10, 8, &[0xbb; 32]));

    assert!(
        fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "evidence headers skip the update-only time rules, so a time below the trusted state freezes rather than reverting"
    );
}

#[test]
fn a_header_whose_time_runs_past_its_later_neighbour_freezes_the_client() {
    let fx = setup();

    apply_update(&fx, 20, 20);

    let message = header_message(&fx.env, 15, 30, &[0u8; 32]);

    assert!(
        fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a header at 15 timed after its neighbour at 20 breaks monotonicity"
    );
}

#[test]
fn a_header_whose_time_falls_below_an_earlier_neighbour_freezes_the_client() {
    let fx = setup();

    apply_update(&fx, 20, 20);

    let message = header_message(&fx.env, 25, 15, &[0u8; 32]);

    assert!(
        fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a header at 25 timed before its neighbour at 20 breaks monotonicity"
    );
}

#[test]
fn a_header_timed_between_its_neighbours_is_not_misbehaviour() {
    let fx = setup();

    apply_update(&fx, 20, 20);

    let message = header_message(&fx.env, 15, 15, &[0u8; 32]);

    assert!(
        !fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a header whose time sits between its neighbours is honest"
    );
}

#[test]
fn a_header_sharing_a_second_with_a_neighbour_is_not_misbehaviour() {
    let fx = setup();

    apply_update(&fx, 20, 20);

    let message = header_message(&fx.env, 15, 20, &[0u8; 32]);

    assert!(
        !fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a header sharing a whole second with its neighbour is not a time violation at second resolution"
    );
}

#[test]
fn a_frozen_client_reports_no_further_misbehaviour() {
    let fx = setup();

    let message = misbehaviour_message(&fx.env, (10, 10, &[0xaa; 32]), (10, 10, &[0xbb; 32]));
    fx.client
        .update_state_on_misbehaviour(&fx.client_id, &message);
    assert!(fx.client.frozen(&fx.client_id));

    assert!(
        !fx.client.check_for_misbehaviour(&fx.client_id, &message),
        "a frozen client short-circuits detection"
    );
}

#[test]
fn anyone_can_submit_misbehaviour_through_the_router() {
    let env = Env::default();
    env.ledger().set_timestamp(NOW);

    let owner = Address::generate(&env);
    let router_address = env.register(IbcRouter, (owner,));
    let router = IbcRouterClient::new(&env, &router_address);

    env.mock_all_auths();

    let light_client = env.register(TendermintLightClient, (router_address.clone(),));
    router.register_client_type(&String::from_str(&env, CLIENT_TYPE), &light_client);

    let client_id = router.create_client(
        &String::from_str(&env, CLIENT_TYPE),
        &client_state(&env),
        &consensus_state(&env),
        &TRUSTED_HEIGHT,
    );

    let message = misbehaviour_message(&env, (10, 10, &[0xaa; 32]), (10, 10, &[0xbb; 32]));

    env.set_auths(&[]);

    assert_eq!(
        router.update_client(&client_id, &message),
        0,
        "a permissionless misbehaviour submission freezes rather than advancing the client"
    );
    assert!(
        router.frozen(&client_id),
        "the router freezes the client on misbehaviour from any submitter"
    );
}

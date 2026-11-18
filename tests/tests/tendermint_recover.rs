use soroban_sdk::{
    testutils::{Address as _, Ledger},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, String,
};
use stellar_tendermint_light_client::{
    Error, Height, TendermintClientState, TendermintConsensusState, TendermintLightClient,
    TendermintLightClientClient, TrustThreshold,
};

const TRUSTING_PERIOD_SECS: u64 = 1_000_000;
const SUBJECT_TIMESTAMP: u64 = 1;
const SUBSTITUTE_TIMESTAMP: u64 = 500;

fn client_state(env: &Env, chain_id: &str, height: u64) -> Bytes {
    TendermintClientState {
        chain_id: String::from_str(env, chain_id),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: TRUSTING_PERIOD_SECS,
        unbonding_period_secs: 2 * TRUSTING_PERIOD_SECS,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: height,
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

fn consensus_state(env: &Env, timestamp: u64, root: u8) -> Bytes {
    TendermintConsensusState {
        timestamp_secs: timestamp,
        next_validators_hash: BytesN::from_array(env, &[0; 32]),
        root: BytesN::from_array(env, &[root; 32]),
    }
    .to_xdr(env)
}

struct Fixture {
    env: Env,
    client: TendermintLightClientClient<'static>,
    subject: String,
    substitute: String,
}

fn setup(subject_chain: &str, substitute_chain: &str) -> Fixture {
    setup_heights(subject_chain, substitute_chain, 10, 20)
}

fn setup_heights(
    subject_chain: &str,
    substitute_chain: &str,
    subject_height: u64,
    substitute_height: u64,
) -> Fixture {
    let env = Env::default();
    let router = Address::generate(&env);
    let address = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(&env, &address);

    let subject = String::from_str(&env, "07-tendermint-0");
    client.initialise(
        &subject,
        &client_state(&env, subject_chain, subject_height),
        &consensus_state(&env, SUBJECT_TIMESTAMP, 0xaa),
        &subject_height,
    );

    let substitute = String::from_str(&env, "07-tendermint-1");
    client.initialise(
        &substitute,
        &client_state(&env, substitute_chain, substitute_height),
        &consensus_state(&env, SUBSTITUTE_TIMESTAMP, 0xbb),
        &substitute_height,
    );

    Fixture {
        env,
        client,
        subject,
        substitute,
    }
}

fn freeze(fx: &Fixture, client_id: &String) {
    fx.client
        .update_state_on_misbehaviour(client_id, &Bytes::new(&fx.env));
}

#[test]
fn a_frozen_client_is_recovered_from_a_substitute() {
    let fx = setup("simd-1", "simd-1");

    freeze(&fx, &fx.subject);
    assert!(fx.client.frozen(&fx.subject));

    fx.client.recover(&fx.subject, &fx.substitute);

    assert!(!fx.client.frozen(&fx.subject));
    assert_eq!(fx.client.latest_height(&fx.subject), 20);
    assert_eq!(
        fx.client.consensus_state(&fx.subject, &20),
        consensus_state(&fx.env, SUBSTITUTE_TIMESTAMP, 0xbb),
        "the subject adopts the substitute's consensus state at its latest height"
    );
}

#[test]
fn an_expired_client_is_recovered_from_a_substitute() {
    let fx = setup("simd-1", "simd-1");
    fx.env
        .ledger()
        .set_timestamp(SUBJECT_TIMESTAMP + TRUSTING_PERIOD_SECS);

    fx.client.recover(&fx.subject, &fx.substitute);

    assert_eq!(fx.client.latest_height(&fx.subject), 20);
    assert_eq!(
        fx.client.consensus_state(&fx.subject, &20),
        consensus_state(&fx.env, SUBSTITUTE_TIMESTAMP, 0xbb),
        "recovery seeds a fresh consensus state within the trusting period again"
    );
}

#[test]
fn recovery_refuses_an_active_subject() {
    let fx = setup("simd-1", "simd-1");

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubjectStillActive.into())),
        "a client that can still be updated has no need of recovery"
    );
    assert_eq!(fx.client.latest_height(&fx.subject), 10);
}

#[test]
fn recovery_refuses_a_substitute_that_is_not_newer() {
    let fx = setup_heights("simd-1", "simd-1", 10, 10);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteNotNewer.into())),
        "a substitute no higher than the subject heals nothing"
    );
}

#[test]
fn recovery_refuses_a_substitute_on_another_chain() {
    let fx = setup("simd-1", "other-1");

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::ChainIdMismatch.into()))
    );
    assert_eq!(
        fx.client.latest_height(&fx.subject),
        10,
        "a refused recovery leaves the subject untouched"
    );
}

#[test]
fn recovery_refuses_a_frozen_substitute() {
    let fx = setup("simd-1", "simd-1");

    freeze(&fx, &fx.subject);
    freeze(&fx, &fx.substitute);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::ClientFrozen.into()))
    );
}

#[test]
fn recovery_refuses_an_expired_substitute() {
    let fx = setup("simd-1", "simd-1");
    freeze(&fx, &fx.subject);
    fx.env
        .ledger()
        .set_timestamp(SUBSTITUTE_TIMESTAMP + TRUSTING_PERIOD_SECS);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteExpired.into()))
    );
}

#[test]
fn only_the_router_can_recover() {
    let fx = setup("simd-1", "simd-1");

    freeze(&fx, &fx.subject);
    fx.env.set_auths(&[]);

    assert!(
        fx.client.try_recover(&fx.subject, &fx.substitute).is_err(),
        "recovery is admin-gated, so only the router reaches it"
    );
}

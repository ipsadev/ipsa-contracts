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
    let env = Env::default();
    let router = Address::generate(&env);
    let address = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(&env, &address);

    let subject = String::from_str(&env, "07-tendermint-0");
    client.initialise(
        &subject,
        &client_state(&env, subject_chain, 10),
        &consensus_state(&env, 1, 0xaa),
        &10,
    );

    let substitute = String::from_str(&env, "07-tendermint-1");
    client.initialise(
        &substitute,
        &client_state(&env, substitute_chain, 20),
        &consensus_state(&env, 500, 0xbb),
        &20,
    );

    Fixture {
        env,
        client,
        subject,
        substitute,
    }
}

#[test]
fn a_frozen_client_is_recovered_from_a_substitute() {
    let fx = setup("simd-1", "simd-1");

    fx.client
        .update_state_on_misbehaviour(&fx.subject, &Bytes::new(&fx.env));
    assert!(fx.client.frozen(&fx.subject));

    fx.client.recover(&fx.subject, &fx.substitute);

    assert!(!fx.client.frozen(&fx.subject));
    assert_eq!(fx.client.latest_height(&fx.subject), 20);
    assert_eq!(
        fx.client.consensus_state(&fx.subject, &20),
        consensus_state(&fx.env, 500, 0xbb),
        "the subject adopts the substitute's consensus state at its latest height"
    );
}

#[test]
fn an_expired_client_is_recovered_from_a_substitute() {
    let fx = setup("simd-1", "simd-1");

    // the subject's only consensus state is at timestamp 1; once the ledger is past
    // the trusting period it can no longer be updated — it is expired.
    fx.env.ledger().set_timestamp(1 + TRUSTING_PERIOD_SECS + 1);

    fx.client.recover(&fx.subject, &fx.substitute);

    assert_eq!(fx.client.latest_height(&fx.subject), 20);
    assert_eq!(
        fx.client.consensus_state(&fx.subject, &20),
        consensus_state(&fx.env, 500, 0xbb),
        "recovery seeds a fresh consensus state within the trusting period again"
    );
}

#[test]
fn recovery_refuses_a_substitute_on_another_chain() {
    let fx = setup("simd-1", "other-1");

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

    fx.client
        .update_state_on_misbehaviour(&fx.substitute, &Bytes::new(&fx.env));

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::ClientFrozen.into()))
    );
}

#[test]
fn only_the_router_can_recover() {
    let fx = setup("simd-1", "simd-1");

    fx.env.set_auths(&[]);

    assert!(
        fx.client.try_recover(&fx.subject, &fx.substitute).is_err(),
        "recovery is admin-gated, so only the router reaches it"
    );
}

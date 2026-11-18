use soroban_sdk::{
    testutils::{Address as _, Ledger},
    xdr::{FromXdr, ToXdr},
    Address, Bytes, BytesN, Env, String,
};
use stellar_tendermint_light_client::{
    Error, Height, TendermintClientState, TendermintConsensusState, TendermintLightClient,
    TendermintLightClientClient, TrustThreshold,
};

const TRUSTING_PERIOD_SECS: u64 = 1_000_000;
const UNBONDING_PERIOD_SECS: u64 = 2 * TRUSTING_PERIOD_SECS;
const CLOCK_DRIFT_SECS: u64 = 10;

const SUBJECT_TIMESTAMP: u64 = 1;
const SUBSTITUTE_TIMESTAMP: u64 = 500;

#[derive(Clone)]
struct Params {
    chain: &'static str,
    height: u64,
    trust: (u32, u32),
    trusting: u64,
    unbonding: u64,
    clock_drift: u64,
    proof_specs: &'static [u8],
}

impl Params {
    fn subject() -> Self {
        Params {
            chain: "simd-1",
            height: 10,
            trust: (1, 3),
            trusting: TRUSTING_PERIOD_SECS,
            unbonding: UNBONDING_PERIOD_SECS,
            clock_drift: CLOCK_DRIFT_SECS,
            proof_specs: &[],
        }
    }

    fn substitute() -> Self {
        Params {
            height: 20,
            ..Params::subject()
        }
    }
}

fn client_state(env: &Env, p: &Params) -> Bytes {
    TendermintClientState {
        chain_id: String::from_str(env, p.chain),
        trust_level: TrustThreshold {
            numerator: p.trust.0,
            denominator: p.trust.1,
        },
        trusting_period_secs: p.trusting,
        unbonding_period_secs: p.unbonding,
        max_clock_drift_secs: p.clock_drift,
        latest_height: Height {
            revision_number: 0,
            revision_height: p.height,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::from_slice(env, p.proof_specs),
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

fn init_pair(subject: Params, substitute: Params) -> Fixture {
    let env = Env::default();
    let router = Address::generate(&env);
    let address = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(&env, &address);

    let subject_id = String::from_str(&env, "07-tendermint-0");
    client.initialise(
        &subject_id,
        &client_state(&env, &subject),
        &consensus_state(&env, SUBJECT_TIMESTAMP, 0xaa),
        &subject.height,
    );

    let substitute_id = String::from_str(&env, "07-tendermint-1");
    client.initialise(
        &substitute_id,
        &client_state(&env, &substitute),
        &consensus_state(&env, SUBSTITUTE_TIMESTAMP, 0xbb),
        &substitute.height,
    );

    Fixture {
        env,
        client,
        subject: subject_id,
        substitute: substitute_id,
    }
}

fn setup() -> Fixture {
    init_pair(Params::subject(), Params::substitute())
}

fn freeze(fx: &Fixture, client_id: &String) {
    fx.client
        .update_state_on_misbehaviour(client_id, &Bytes::new(&fx.env));
}

fn decode(fx: &Fixture, client_id: &String) -> TendermintClientState {
    TendermintClientState::from_xdr(&fx.env, &fx.client.client_state(client_id)).unwrap()
}

#[test]
fn a_frozen_client_is_recovered_from_a_substitute() {
    let fx = setup();

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
    let fx = setup();

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
fn recovery_adopts_the_substitutes_trusting_period() {
    let substitute = Params {
        trusting: TRUSTING_PERIOD_SECS / 2,
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);
    fx.client.recover(&fx.subject, &fx.substitute);

    assert_eq!(
        decode(&fx, &fx.subject).trusting_period_secs,
        TRUSTING_PERIOD_SECS / 2,
        "the subject takes the substitute's trusting period"
    );
}

#[test]
fn recovery_refuses_an_active_subject() {
    let fx = setup();

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubjectStillActive.into())),
        "a client that can still be updated has no need of recovery"
    );
    assert_eq!(fx.client.latest_height(&fx.subject), 10);
}

#[test]
fn recovery_refuses_a_substitute_that_is_not_newer() {
    let substitute = Params {
        height: 10,
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteNotNewer.into())),
        "a substitute no higher than the subject heals nothing"
    );
}

#[test]
fn recovery_refuses_a_substitute_on_another_chain() {
    let substitute = Params {
        chain: "other-1",
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

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
    let fx = setup();

    freeze(&fx, &fx.subject);
    freeze(&fx, &fx.substitute);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::ClientFrozen.into()))
    );
}

#[test]
fn recovery_refuses_an_expired_substitute() {
    let fx = setup();

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
fn recovery_refuses_a_substitute_with_a_different_trust_level() {
    let substitute = Params {
        trust: (2, 3),
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteParamsMismatch.into())),
        "a substitute verified under a weaker trust level cannot stand in"
    );
}

#[test]
fn recovery_refuses_a_substitute_with_a_different_unbonding_period() {
    let substitute = Params {
        unbonding: UNBONDING_PERIOD_SECS + 1,
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteParamsMismatch.into()))
    );
}

#[test]
fn recovery_refuses_a_substitute_with_a_different_clock_drift() {
    let substitute = Params {
        clock_drift: CLOCK_DRIFT_SECS + 1,
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteParamsMismatch.into()))
    );
}

#[test]
fn recovery_refuses_a_substitute_with_different_proof_specs() {
    let substitute = Params {
        proof_specs: &[1, 2, 3],
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::SubstituteParamsMismatch.into()))
    );
}

#[test]
fn recovery_refuses_a_trusting_period_not_below_the_unbonding_period() {
    let substitute = Params {
        trusting: UNBONDING_PERIOD_SECS,
        ..Params::substitute()
    };
    let fx = init_pair(Params::subject(), substitute);

    freeze(&fx, &fx.subject);

    assert_eq!(
        fx.client.try_recover(&fx.subject, &fx.substitute),
        Err(Ok(Error::TrustingPeriodNotBelowUnbonding.into())),
        "a trusting period at or above the unbonding period is unsafe"
    );
}

#[test]
fn only_the_router_can_recover() {
    let fx = setup();

    freeze(&fx, &fx.subject);
    fx.env.set_auths(&[]);

    assert!(
        fx.client.try_recover(&fx.subject, &fx.substitute).is_err(),
        "recovery is admin-gated, so only the router reaches it"
    );
}

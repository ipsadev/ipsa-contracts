use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};
use stellar_ibc_router::{IbcRouter, IbcRouterClient};
use stellar_mock_light_client::MockLightClient;

pub fn setup() -> (Env, Address, IbcRouterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin.clone(),));
    let router = IbcRouterClient::new(&env, &router_id);

    let lc_id = env.register(MockLightClient, ());

    router.register_client_type(&String::from_str(&env, "mock"), &lc_id);

    (env, router_id, router, lc_id)
}

#[allow(dead_code)]
pub fn router_roots(env: &Env, router_id: &Address) -> std::vec::Vec<[u8; 32]> {
    use soroban_sdk::testutils::Events;
    use soroban_sdk::xdr::{ContractEventBody, ScVal};

    let mut out = std::vec::Vec::new();
    for event in env.events().all().filter_by_contract(router_id).events() {
        let ContractEventBody::V0(body) = &event.body;
        if body.topics.len() != 1 {
            continue;
        }
        let ScVal::Symbol(symbol) = &body.topics[0] else {
            continue;
        };
        if symbol.0.as_slice() != b"ibc_root" {
            continue;
        }
        let ScVal::Bytes(bytes) = &body.data else {
            panic!("ibc_root data must be SCVal::Bytes, got {:?}", body.data);
        };
        out.push(
            bytes
                .as_slice()
                .try_into()
                .expect("ibc_root data must be exactly 32 bytes"),
        );
    }
    out
}

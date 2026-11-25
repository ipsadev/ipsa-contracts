use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};
use stellar_ibc_gmp::{IbcGmpApp, IbcGmpAppClient};

#[test]
fn the_dev_kit_derives_the_same_account_as_the_gmp_app() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let router = Address::generate(&env);
    let gmp = env.register(IbcGmpApp, (owner, router));
    let app = IbcGmpAppClient::new(&env, &gmp);

    for (client, sender, salt) in [
        (
            "07-tendermint-0",
            "cosmos1nvhvm94jnpmltlspnemwlgmzf9lemmjm924yw9",
            &b""[..],
        ),
        (
            "sepolia-0",
            "0x0000000000000000000000000000000000000001",
            &[7u8][..],
        ),
        (
            "08-wasm-5",
            "GAPCJ2GXGIY2VRLK6W4DDY443R3JDMJ3K5LQBMUJL4LDSX34MXQMSHVD",
            &[1, 2, 3][..],
        ),
    ] {
        let client = String::from_str(&env, client);
        let sender = String::from_str(&env, sender);
        let salt = Bytes::from_slice(&env, salt);

        assert_eq!(
            ipsa::gmp::remote_account(&env, &gmp, &client, &sender, &salt),
            app.account_address(&client, &sender, &salt).address
        );
    }
}

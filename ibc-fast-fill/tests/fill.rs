mod common;

use common::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, String,
};
use stellar_ibc_fast_fill::Error;

#[test]
fn a_filled_order_pays_the_recipient_at_once_and_the_packet_repays_the_provider() {
    let f = setup();
    let order = order(&f, 1);

    let key = f.fast_fill.fill(&f.provider, &order, &provider_evm(&f));

    assert_eq!(balance(&f, &f.recipient), AMOUNT - FEE);
    assert_eq!(balance(&f, &f.provider), 9 * AMOUNT + FEE);
    assert!(!f.fast_fill.is_settled(&key));

    deliver(&f, &order);

    assert_eq!(balance(&f, &f.recipient), AMOUNT - FEE);
    assert_eq!(balance(&f, &f.provider), 10 * AMOUNT + FEE);
    assert_eq!(balance(&f, &f.fast_fill_addr), 0);
    assert!(f.fast_fill.is_settled(&key));
    assert_eq!(
        f.router.acknowledgement(&f.stellar_client, &1),
        Some(success_ack(&f))
    );
}

#[test]
fn an_unfilled_order_pays_the_recipient_when_the_packet_lands() {
    let f = setup();
    let order = order(&f, 1);

    deliver(&f, &order);

    assert_eq!(balance(&f, &f.recipient), AMOUNT);
    assert_eq!(balance(&f, &f.provider), 10 * AMOUNT);
    assert_eq!(
        f.router.acknowledgement(&f.stellar_client, &1),
        Some(success_ack(&f))
    );
}

#[test]
fn an_order_already_delivered_cannot_be_filled() {
    let f = setup();
    let order = order(&f, 1);

    deliver(&f, &order);

    let result = f.fast_fill.try_fill(&f.provider, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::AlreadySettled.into()));
    assert_eq!(balance(&f, &f.provider), 10 * AMOUNT);
}

#[test]
fn an_order_is_filled_at_most_once() {
    let f = setup();
    let order = order(&f, 1);
    let second = Address::generate(&f.env);

    soroban_sdk::token::StellarAssetClient::new(&f.env, &f.voucher).mint(&second, &AMOUNT);

    f.fast_fill.fill(&f.provider, &order, &provider_evm(&f));

    let result = f.fast_fill.try_fill(&second, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::AlreadyFilled.into()));
    assert_eq!(balance(&f, &second), AMOUNT);
}

#[test]
fn a_fill_that_misstates_the_order_is_never_repaid() {
    let f = setup();
    let real = order(&f, 1);
    let mut misstated = real.clone();

    misstated.memo = memo_for(&f, &f.provider, FEE, NOW + 600);

    f.fast_fill.fill(&f.provider, &misstated, &provider_evm(&f));

    deliver(&f, &real);

    assert_eq!(balance(&f, &f.recipient), AMOUNT);
    assert_eq!(balance(&f, &f.provider), 10 * AMOUNT);
}

#[test]
fn a_fill_after_the_deadline_is_refused() {
    let f = setup();
    let order = order(&f, 1);

    f.env.ledger().set_timestamp(NOW + 601);

    let result = f.fast_fill.try_fill(&f.provider, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::FillDeadlinePassed.into()));
}

#[test]
fn a_fee_must_be_below_the_amount() {
    let f = setup();
    let order = order_with(&f, 1, AMOUNT, memo_for(&f, &f.recipient, AMOUNT, NOW + 600));

    let result = f.fast_fill.try_fill(&f.provider, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::InvalidFee.into()));
}

#[test]
fn a_token_without_a_route_cannot_be_filled() {
    let f = setup();
    let mut order = order(&f, 1);

    order.denom = String::from_str(&f.env, "0x3333333333333333333333333333333333333333");

    let result = f.fast_fill.try_fill(&f.provider, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::UnknownRoute.into()));
}

#[test]
fn an_order_addressed_to_another_contract_cannot_be_filled() {
    let f = setup();
    let elsewhere = Address::generate(&f.env);
    let memo = String::from_str(
        &f.env,
        &format!(
            "{{\"dest_callback\":{{\"address\":\"{}\"}},\"fast_fill\":\"{}:{FEE}:{}\"}}",
            elsewhere.to_string(),
            f.recipient.to_string(),
            NOW + 600
        ),
    );
    let order = order_with(&f, 1, AMOUNT, memo);

    let result = f.fast_fill.try_fill(&f.provider, &order, &provider_evm(&f));

    assert!(matches!(result, Err(Ok(error)) if error == Error::NotAddressedHere.into()));
}

#[test]
fn a_provider_must_give_a_valid_ethereum_address() {
    let f = setup();
    let order = order(&f, 1);

    let result = f
        .fast_fill
        .try_fill(&f.provider, &order, &String::from_str(&f.env, "0x12"));

    assert!(matches!(result, Err(Ok(error)) if error == Error::InvalidEvmAddress.into()));
}

#[test]
fn a_packet_without_fill_terms_is_refunded() {
    let f = setup();
    let memo = String::from_str(
        &f.env,
        &format!(
            "{{\"dest_callback\":{{\"address\":\"{}\"}}}}",
            f.fast_fill_addr.to_string()
        ),
    );
    let order = order_with(&f, 1, AMOUNT, memo);

    deliver(&f, &order);

    assert_eq!(balance(&f, &f.fast_fill_addr), 0);
    assert_eq!(
        f.router.acknowledgement(&f.stellar_client, &1),
        Some(error_ack(&f))
    );
}

#[test]
fn only_the_transfer_app_can_report_a_received_transfer() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    let received = ipsa::ics20::TransferReceived {
        source_client: f.sepolia_client.clone(),
        dest_client: f.stellar_client.clone(),
        sequence: 1,
        sender: String::from_str(&f.env, FAST_TRANSFER),
        token: f.voucher.clone(),
        amount: AMOUNT,
        memo: memo_for(&f, &f.recipient, FEE, NOW + 600),
    };

    let result = f
        .fast_fill
        .try_on_ibc_transfer_received(&impostor, &received);

    assert!(matches!(result, Err(Ok(error)) if error == Error::Unauthorized.into()));
}

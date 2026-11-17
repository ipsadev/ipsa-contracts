#[cfg(test)]
#[path = "../../ics23/tests/common/mod.rs"]
#[allow(dead_code)]
mod ics23_fixtures;

#[cfg(test)]
mod e2e;

#[cfg(test)]
mod ics23_tendermint;

#[cfg(test)]
mod tendermint_verify;

#[cfg(test)]
mod soroban_budget_spikes;

#[cfg(test)]
mod light_client_access;

#[cfg(test)]
mod router_proofs;

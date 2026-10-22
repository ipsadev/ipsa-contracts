# `ipsa-gmp-caller` — a contract that sends GMP calls and records their outcome

An example of the sending side of ICS-27 GMP on Soroban, and the contract the ipsa CLI uses to test it.

`send_call` is owner-only and sends the call through the GMP app with this contract as the sender, so the GMP app reports the outcome back to it. It records `Pending` under the source client and sequence, and the callbacks replace that with the outcome:

- `on_gmp_acknowledgement(gmp_app, result)` records `Acknowledged`, with `success` and the raw acknowledgement, which on success holds the destination call's result.
- `on_gmp_timeout(gmp_app, result)` records `TimedOut`.

Both refuse any caller other than the GMP app the contract was constructed with, and require its authorisation, which holds only when that app is the direct caller. `outcome(source_client_id, sequence)` reads what was recorded.

The constructor takes the owner and the GMP app's address. The tests in `tests/outcomes.rs` run the whole path through a router and the GMP app: pending, success, failure, timeout, and a refused impostor.

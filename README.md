# Constant-product AMM

This repository contains the AMM homework implementation. It is an Anchor program that
supports two SPL tokens, PDA-owned pool vaults, LP share accounting, constant-product
swaps, configurable fees, and a treasury account.

## Instructions

| Instruction | Behavior |
| --- | --- |
| `initialize_pool(fee_bps, initial_a, initial_b)` | Creates the pool, vaults, LP mint, treasury token accounts, deposits the initial reserves, and mints the initial LP shares. |
| `add_liquidity(amount_a, amount_b, min_shares)` | Deposits both tokens and records/mints proportional LP shares. |
| `remove_liquidity(shares, min_a, min_b)` | Burns LP shares and withdraws both reserves using the pool PDA as signer. |
| `swap(input_is_a, amount_in, min_amount_out)` | Quotes output with `x * y = k`, transfers the fee to the treasury, and enforces the minimum output. |
| `collect_fees(input_is_a, amount)` | Lets only the configured treasury signer withdraw accumulated fees to another treasury-owned token account. |

Fees are expressed in basis points and capped at 10% (`MAX_FEE_BPS = 1_000`). The
default test example uses 30 bps. Arithmetic uses checked operations and every user-facing
amount has a zero-amount or slippage guard.

## Run the tests

This project is pinned to the Anchor 0.30.1 / Solana 1.18 dependency family. That
alignment matters: Anchor's SBF builder uses the Rust compiler bundled with the installed
Solana CLI, and newer Solana 2.x crates can require a newer Rust compiler even when the
host machine has a newer Rust installed. Keep the Anchor CLI, Solana CLI, `anchor-lang`,
`anchor-spl`, and the lockfile on one compatible version family.

Install Anchor 0.30.1, a matching Solana 1.18.x CLI, Rust, and Node.js, then run:

```bash
cargo fmt --all -- --check
cargo test --workspace
anchor build
anchor test
```

With AVM-managed Anchor installations, select the version declared in
`Anchor.toml` first (`avm use 0.30.1`). The checked-in
`docs/test-output.txt` contains the verified passing output from the legacy
Solana toolchain used for this submission.

The Rust tests validate the CPMM quote and share math. `tests/amm.ts` is the Anchor
integration-test checklist covering initialization, adding/removing liquidity, both swap
directions, slippage failures, and treasury fee collection. Run `anchor test` against a
local validator to produce the requested terminal screenshot of the passing suite. If your
machine has an older Solana CLI whose bundled Cargo rejects the lockfile, use a v3
`Cargo.lock` (already committed here) and do not regenerate it with a newer Cargo unless
you also upgrade the Solana/Anchor toolchain together.

The compatibility diagnosis and the vendored `anchor-syn` IDL patch are documented in
[`docs/TOOLCHAIN_SETUP.md`](docs/TOOLCHAIN_SETUP.md). Do not add global
`procmacro2_semver_exempt` flags: they can break unrelated proc macros.

## Downtime mitigation

DeFi applications should treat the chain, RPC, and frontend as separate failure domains:

1. Use multiple RPC providers with health checks, bounded timeouts, and exponential backoff.
2. Read from a second provider when the primary is stale, and show the slot/commitment in the UI.
3. Keep transactions retryable and idempotent; never blindly duplicate a swap after an unknown
   confirmation.
4. Include slippage and deadline protections, simulate before sending, and surface failures.
5. Monitor program error rates, vault balances, oracle/RPC lag, and validator health, with a
   runbook for pausing frontend submissions while on-chain state remains auditable.
6. Cache only immutable metadata and rebuild all balances/quotes from a fresh confirmed state
   after reconnecting.

## Security notes

Pool vaults and the LP mint are controlled by the pool PDA, not by the wallet that created
the pool. Account constraints bind every token account to its expected mint and authority.
The treasury is stored immutably in the pool state, and only that signer can collect fees.

# ADR-0010: Multi-asset member exit payouts

- **Status:** Proposed
- **Date:** 2026-11-21

## Context

The DAO now holds multiple whitelisted assets, but `calculate_exit_share`
currently computes a member's share from the base-token treasury only. A
member leaving can therefore receive no value for the non-base assets owned by
the treasury. Returning an arbitrary basket also changes `exit_dao` from one
token transfer into a coordinated set of transfers whose accounting and
failure behavior must be defined.

## Decision

Recommend that an exiting member receive a pro-rata basket in every asset held
by the treasury. For each asset, calculate the member's share using the
existing contribution weighting and treasury-favouring floor division:

`floor(asset treasury balance * member contribution / active contributions)`.

The exit is atomic: settle the member's share and any asset-specific yield,
update membership state, then transfer each positive payout in its own asset.
If any transfer fails, the transaction must revert as a whole. Staked base
tokens remain part of the existing base-token exit accounting; this decision
does not redefine staking.

For example, with two equally contributing active members and treasury
balances of 600 base tokens and 300 units of asset B, each member's basket
share is 300 base tokens and 150 units of B. If one member exits, they receive
both amounts, subject to floor rounding and any separately defined stake and
yield settlement.

## Alternatives considered

- **Base token only:** simple and ABI-compatible, but permanently strands each
  exiting member's proportional claim on non-base treasury assets.
- **Member-selected asset:** lets a member choose a payout denomination, but
  requires a conversion or liquidity source that the DAO does not provide.
  Selecting one of the held tokens instead would require a price oracle and a
  slippage policy, and could let early exits consume the most liquid assets.
- **Member-selected subset of the basket:** adds partial-exit accounting and
  leaves residual claims that complicate membership status and future exits.

## Consequences

- `exit_dao` will need to account for and transfer a payout per whitelisted
  asset, while preserving atomic settlement and treasury-favouring rounding.
- The current scalar `calculate_exit_share` remains a base-token view; an
  implementation must add an asset-specific quote view or a basket view rather
  than changing the existing return type.
- The implementation must define how removed assets with outstanding funds
  remain reachable. `remove_token` should not permit removing an asset while
  the treasury still holds its balance.
- A follow-up implementation issue should cover the basket payout, its views,
  transfer-failure behavior, and multi-asset exit tests. This issue is a
  design proposal only; maintainer sign-off is required before implementation.

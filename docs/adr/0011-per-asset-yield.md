# ADR-0011: Per-asset yield accounting

- **Status:** Proposed
- **Date:** 2026-11-21

## Context

Loan interest is denominated in the asset used for the loan. The current
`YieldAccumulator` and `YieldRemainder` are global, and member snapshots are
not asset-scoped. Crediting interest repaid in asset B to the base-token
accumulator misstates each member's claim and can cause rewards to be paid in
the wrong token. Assets may also have different decimal scales, so balances
must never be combined across assets.

## Decision

Track each accumulator, its rounding remainder, and each member's snapshot by
asset:

- `YieldAccumulator(Address)` for per-asset cumulative yield per active member.
- `YieldRemainder(Address)` for per-asset indivisible interest.
- `MemberYieldSnapshot(Address, Address)` for the member's per-asset snapshot.

When interest is repaid, add it only to the accumulator for the loan's asset.
For each asset, divide interest plus that asset's carried remainder among the
active members; retain the division remainder in the same asset. Pending yield
is the non-negative difference between that asset's accumulator and the
member's snapshot. Claim and exit settlement must transfer each amount in its
own asset; no exchange rate or cross-asset conversion is implied.

For example, with three active members, a repayment of 11 units of asset B
credits 3 units to each member's B yield and carries 2 units of B remainder.
The base-token accumulator and each member's base-token pending yield remain
unchanged. A later base-token repayment is divided and rounded independently.

## Alternatives considered

- **One global accumulator:** rejected because it attributes interest in one
  token to claims paid in another token, and produces meaningless totals when
  tokens have different decimal scales.
- **Convert every repayment to the base token:** rejected because the contract
  has no oracle or conversion facility and would introduce price, slippage, and
  availability assumptions.
- **Track a separate yield balance per member:** rejected because it makes
  repayments and claims scale with membership instead of preserving the
  existing pull-based accumulator design.

## Migration

Do not rewrite or delete existing storage. Keep the legacy singleton
accumulator, remainder, and one-address member-snapshot keys readable for the
configured base token. For a base-token read, prefer the new asset-scoped key
when it exists and otherwise fall back to the legacy key. The first subsequent
write stores the value under the new key; thereafter that key takes precedence.
This lazy migration preserves existing base-token claims without a one-time
loop over members or a migration transaction.

Non-base asset keys start at zero. No historical per-asset yield was recorded,
so there is no non-base accumulator or snapshot to migrate. Use the same
asset-scoped key consistently for each repayment, pending-yield view, claim,
and exit. Persistent member snapshots and remainders must continue to extend
their storage TTL on access and write.

## Consequences

- Base-token interest accounting remains unchanged, including accrued legacy
  values and rounding remainders.
- Asset B repayment can no longer increase base-token yield. Clients need an
  asset-specific pending-yield view and claim path; the existing base-token
  entrypoints should remain compatible.
- `exit_dao` must settle yield separately for each asset. The basket treatment
  of a member's capital share is specified independently by ADR-0010.
- Add a regression test that repays a loan in asset B and proves base pending
  yield is unchanged while asset-B pending yield increases. This test belongs
  with the implementation follow-up (#271), so the ADR-only change does not
  introduce a knowingly failing CI test.

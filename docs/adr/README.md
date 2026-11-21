# Architecture Decision Records

ADRs capture decisions that affect the contract’s architecture, public ABI,
security model, or operational lifecycle. Accepted decisions are immutable
history: supersede an old record with a new ADR rather than rewriting it.

## Template

Copy [`TEMPLATE.md`](./TEMPLATE.md), choose the next number, and add the
record to the index below. Keep the status (`Proposed`, `Accepted`,
`Superseded`, or `Deprecated`) and link the superseding ADR when applicable.

## Review Gate for ADR Status Changes (#520)

Per ADR-0001, accepted decisions are immutable architecture history. Moving an ADR's status from `Proposed` to `Accepted` requires explicit sign-off from at least two project maintainers recorded in the approving PR review. Changes to `docs/adr/` are protected via `.github/CODEOWNERS`.

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](./0001-immutable-contract-vs-upgradeable.md) | Immutable contract vs upgradeable contract | Accepted |
| [0004](./0004-loan-extension.md) | Loan due-date extension proposals | Proposed |
| [0005](./0005-reputation-rate.md) | Interest discount from member loan history | Proposed |
| [0008](./0008-early-repayment.md) | Early repayment discount | Proposed |
| [0009](./0009-max-open-proposals-policy.md) | Evaluation of MAX_OPEN_PROPOSALS_PER_MEMBER in LoanPolicy | Proposed |
| [0011](./0011-per-asset-yield.md) | Per-asset yield accounting | Proposed |


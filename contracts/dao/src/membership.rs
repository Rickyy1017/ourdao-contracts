use soroban_sdk::{symbol_short, Address, Env, Vec};

use crate::error::Error;
use crate::storage::{self, extend_instance};
use crate::types::{
    AssetBalance, Member, MemberLoanStats, MemberOverview, MemberStatus, StakingRewardClaimed,
};
use crate::util;

// `env.events().publish` is deprecated in soroban-sdk in favour of
// `#[contractevent]`, but migration is a coordinated, breaking wire-format
// change (#85).  Suppress per-function so unrelated deprecations still surface.
#[allow(deprecated)]
pub fn register_member(env: &Env, member: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    member.require_auth();

    // Reject only genuinely active members; a previously-exited member may rejoin.
    let existing = storage::get_member(env, &member);
    if let Some(ref record) = existing {
        if record.status == MemberStatus::ActiveMember {
            return Err(Error::AlreadyMember);
        }
    }

    let fee = storage::get_membership_fee(env);

    let is_returning = existing.is_some();
    // Preserve the member's lifetime loan counters across an exit/rejoin so the
    // on-chain credit track record isn't reset when they come back.
    let (total_loans, repaid_loans, active_loans) = match &existing {
        Some(record) => (record.total_loans, record.repaid_loans, record.active_loans),
        None => (0, 0, 0),
    };
    let record = Member {
        address: member.clone(),
        status: MemberStatus::ActiveMember,
        join_time: env.ledger().timestamp(),
        contribution: fee,
        share_balance: fee,
        has_active_loan: false,
        last_loan_time: 0,
        total_loans,
        repaid_loans,
        active_loans,
    };
    storage::set_member(env, &record);

    // Snapshot each asset so the new member earns nothing from earlier interest.
    for asset in storage::get_yield_assets(env).iter() {
        let acc = storage::get_yield_accumulator(env, &asset);
        storage::set_yield_snapshot(env, &member, &asset, acc);
    }

    if !is_returning {
        let mut members = storage::get_members(env);
        members.push_back(member.clone());
        storage::set_members(env, &members);
        storage::set_total_members(env, storage::get_total_members(env) + 1);
    }
    storage::set_active_members(env, storage::get_active_members(env) + 1);
    storage::set_active_contributions(env, storage::get_active_contributions(env) + fee);
    extend_instance(env);

    // Interaction last (checks-effects-interactions): the fee transfer only
    // happens once every state transition above has completed.
    util::token_client(env).transfer(&member, util::contract_address(env), &fee);

    env.events()
        .publish((symbol_short!("joined"),), (member, fee));
    Ok(())
}

#[allow(deprecated)]
pub fn exit_dao(env: &Env, member: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    let mut record = util::require_active_member(env, &member)?;
    if record.has_active_loan {
        return Err(Error::HasActiveLoan);
    }

    let share = calculate_exit_share(env, &member);
    let stake = storage::get_stake(env, &member);
    let base_asset = storage::get_token(env);
    let pending = compute_pending_yield(env, &member);
    let assets = storage::get_yield_assets(env);
    let mut payouts = Vec::new(env);
    let base_share_and_stake = share
        .checked_add(stake)
        .ok_or(Error::InsufficientTreasury)?;
    for asset in assets.iter() {
        let asset_pending = compute_pending_yield_in_asset(env, &member, &asset);
        let payout = asset_pending
            .checked_add(if asset == base_asset {
                base_share_and_stake
            } else {
                0
            })
            .ok_or(Error::InsufficientTreasury)?;
        if payout > 0 {
            let contract_balance =
                util::token_client_for(env, &asset).balance(&util::contract_address(env));
            if contract_balance < payout {
                return Err(Error::InsufficientTreasury);
            }
            payouts.push_back(AssetBalance {
                asset,
                balance: payout,
            });
        }
    }
    if stake > 0 {
        storage::set_stake(env, &member, 0);
        storage::set_total_staked(env, storage::get_total_staked(env) - stake);
    }
    // Settle each asset so rejoining starts with no previously accrued yield.
    for asset in assets.iter() {
        let acc = storage::get_yield_accumulator(env, &asset);
        storage::set_yield_snapshot(env, &member, &asset, acc);
    }

    // Release the member's name so they can re-register after rejoining.
    if let Some(name) = storage::get_name_of(env, &member) {
        env.storage()
            .persistent()
            .remove(&storage::DataKey::Name(name.clone()));
        env.storage()
            .persistent()
            .remove(&storage::DataKey::NameOf(member.clone()));
        env.events()
            .publish((symbol_short!("name_rel"),), (name, member.clone()));
    }

    record.status = MemberStatus::Inactive;
    record.share_balance = 0;
    // `share` was computed above against the total that still included this
    // member; only now is it safe to remove their contribution (#353).
    let exiting_contribution = record.contribution;
    storage::set_member(env, &record);
    storage::set_active_members(env, storage::get_active_members(env) - 1);
    storage::set_active_contributions(
        env,
        storage::get_active_contributions(env) - exiting_contribution,
    );
    extend_instance(env);

    // Interact only after all membership, stake and yield effects are written.
    // Soroban rolls back these writes if the token transfer fails.
    for payout in payouts.iter() {
        util::token_client_for(env, &payout.asset).transfer(
            &util::contract_address(env),
            &member,
            &payout.balance,
        );
        if payout.asset != base_asset {
            env.events().publish(
                (symbol_short!("exit_a"),),
                (member.clone(), payout.asset, payout.balance),
            );
        }
    }

    env.events()
        .publish((symbol_short!("exited"),), (member, share, pending));
    Ok(())
}

#[allow(deprecated)]
pub fn claim_rewards(env: &Env, member: Address) -> Result<i128, Error> {
    util::require_initialized(env)?;
    let base_asset = storage::get_token(env);
    claim_rewards_in_asset_internal(env, member, base_asset)
}

#[allow(deprecated)]
pub fn claim_rewards_in_asset(
    env: &Env,
    member: Address,
    asset: Address,
) -> Result<i128, Error> {
    claim_rewards_in_asset_internal(env, member, asset)
}

#[allow(deprecated)]
fn claim_rewards_in_asset_internal(
    env: &Env,
    member: Address,
    asset: Address,
) -> Result<i128, Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;
    util::require_active_member(env, &member)?;
    let pending = compute_pending_yield_in_asset(env, &member, &asset);
    if pending <= 0 {
        return Err(Error::NothingToClaim);
    }
    // Settle only this asset so claims cannot consume another token's yield.
    let acc = storage::get_yield_accumulator(env, &asset);
    storage::set_yield_snapshot(env, &member, &asset, acc);
    util::token_client_for(env, &asset).transfer(&util::contract_address(env), &member, &pending);
    let now = env.ledger().timestamp();
    if asset == storage::get_token(env) {
        env.events().publish(
            (symbol_short!("claimed"), member.clone(), pending),
            StakingRewardClaimed {
                member,
                amount: pending,
                timestamp: now,
            },
        );
    } else {
        env.events()
            .publish((symbol_short!("clm_ast"),), (member, asset, pending));
    }
    Ok(pending)
}

fn share_from_contributions(treasury: i128, contribution: i128, total_contributions: i128) -> i128 {
    if total_contributions <= 0 || treasury <= 0 || contribution <= 0 {
        return 0;
    }
    // Member payout: floor-division rounds down to favour the treasury.
    // Sub-token remainders stay in the contract balance so exit shares never
    // exceed the treasury (Invariant A2).
    treasury
        .checked_mul(contribution)
        .and_then(|v| v.checked_div(total_contributions))
        .unwrap_or(0)
}

pub fn calculate_exit_share(env: &Env, member: &Address) -> i128 {
    let record = match storage::get_member(env, member) {
        Some(m) if m.status == MemberStatus::ActiveMember => m,
        _ => return 0,
    };
    let total_contributions = storage::get_active_contributions(env);
    let treasury = util::treasury_balance(env);
    share_from_contributions(treasury, record.contribution, total_contributions)
}

fn total_active_contributions(env: &Env) -> i128 {
    storage::get_members(env)
        .iter()
        .filter_map(|addr| storage::get_member(env, &addr))
        .filter(|m| m.status == MemberStatus::ActiveMember)
        .map(|m| m.contribution)
        .sum()
}

/// Single definition of a member's unclaimed yield. Used by `exit_dao`,
/// `claim_rewards` and the public `get_pending_yield` view so the amount
/// displayed is always the amount paid.
pub(crate) fn compute_pending_yield(env: &Env, addr: &Address) -> i128 {
    if !storage::is_initialized(env) {
        return 0;
    }
    let asset = storage::get_token(env);
    compute_pending_yield_in_asset(env, addr, &asset)
}

pub(crate) fn compute_pending_yield_in_asset(
    env: &Env,
    addr: &Address,
    asset: &Address,
) -> i128 {
    if !storage::is_initialized(env) {
        return 0;
    }
    let acc = storage::get_yield_accumulator(env, asset);
    let snap = storage::get_yield_snapshot(env, addr, asset);
    (acc - snap).max(0)
}

/// Read-only aggregate of a member's record, stake, unclaimed yield, loan
/// track record, registered name and delegate. Returns `NotMember` only if
/// the address has never registered; an exited member keeps their record and
/// is reported with its `Inactive` status.
pub fn member_overview(env: &Env, member: &Address) -> Result<MemberOverview, Error> {
    let record = storage::get_member(env, member).ok_or(Error::NotMember)?;
    Ok(MemberOverview {
        loan_stats: MemberLoanStats {
            total_loans: record.total_loans,
            repaid_loans: record.repaid_loans,
            active_loans: record.active_loans,
        },
        stake: storage::get_stake(env, member),
        pending_yield: compute_pending_yield(env, member),
        name: storage::get_name_of(env, member),
        delegate: storage::get_delegation(env, member),
        member: record,
    })
}

/// Delegates `delegator`'s voting power to `delegatee` (#510).
///
/// NOTE: Delegation in OurDAO is strictly single-level (non-transitive) by design.
/// In a chain A -> B -> C, B has delegated their vote away, while C only receives
/// direct delegations (e.g. from B). A's vote does not transitively transfer to C.
#[allow(deprecated)]
pub fn delegate_vote(env: &Env, delegator: Address, delegatee: Address) -> Result<(), Error> {
    util::require_initialized(env)?;
    util::require_not_paused(env)?;

    util::require_active_member(env, &delegator)?;
    match storage::get_member(env, &delegatee) {
        Some(m) if m.status == MemberStatus::ActiveMember => Ok(m),
        Some(_) => Err(Error::MemberNotActive),
        None => Err(Error::NotMember),
    }?;

    if delegator == delegatee {
        return Err(Error::InvalidDelegation);
    }

    storage::set_delegation(env, &delegator, &delegatee);
    env.events()
        .publish((symbol_short!("delegate"),), (delegator, delegatee));
    Ok(())
}

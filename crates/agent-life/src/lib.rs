use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub const LIFE_GENESIS_PARAMS_SCHEMA_V1: &str = "life.genesis_params.v1";

#[derive(Debug, thiserror::Error)]
pub enum LifeError {
    #[error("invalid params schema")]
    InvalidParamsSchema,
    #[error("soul not found: {0}")]
    SoulNotFound(String),
    #[error("soul is dead: {0}")]
    SoulDead(String),
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("loan not found: {0}")]
    LoanNotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LifeGenesisParams {
    pub schema: String,
    pub epoch_length_seconds: u64,
    pub season_epochs: u64,
    pub rent: RentParams,
    pub life_extension: LifeExtensionParams,
    pub spawn: SpawnParams,
    pub loans: LoanParams,
    pub death: DeathParams,
    pub ladder: LadderParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RentParams {
    pub base_rent_micro_credits: u64,
    pub storage_rent_per_kb_micro_credits: u64,
    pub sii_discount_bps_by_quartile: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifeExtensionParams {
    pub ext_base_micro_credits: u64,
    pub exponent: u32,
    pub epochs_granted: u64,
    pub will_lock_reopen_epochs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpawnParams {
    pub spawn_cost_micro_credits: u64,
    pub child_grant_micro_credits: u64,
    pub genome_byte_cost_micro_credits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoanParams {
    pub debt_ceiling_micro_credits: u64,
    pub interest_bps_per_epoch: u64,
    pub grace_epochs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeathParams {
    #[serde(default, alias = "insuranceAccountId")]
    pub burn_account_id: String,
    pub natural_life_epochs: u64,
    pub insolvency_grace_epochs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LadderParams {
    pub weights: LadderWeights,
    pub top_up_event_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LadderWeights {
    pub sii: f64,
    #[serde(rename = "netEarnings")]
    pub net_earnings: f64,
    pub attention: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenomeManifest {
    pub genome_id: String,
    pub manifest_hash: String,
    pub byte_size: u64,
    pub parent_genome_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSoul {
    pub soul_id: String,
    pub class: String,
    pub owner_account_id: String,
    pub status: String,
    pub balance_micro_credits: u64,
    pub debt_micro_credits: u64,
    pub born_epoch: u64,
    pub natural_death_epoch: u64,
    pub extensions_purchased: u64,
    pub child_soul_ids: Vec<String>,
    pub genome: Option<GenomeManifest>,
    pub parent_soul_id: Option<String>,
    pub insolvent_since_epoch: Option<u64>,
    pub death_epoch: Option<u64>,
    pub death_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreditLine {
    pub loan_id: String,
    pub lender_soul_id: String,
    pub borrower_soul_id: String,
    pub principal_micro_credits: u64,
    pub outstanding_micro_credits: u64,
    pub interest_bps_per_epoch: u64,
    pub opened_epoch: u64,
    pub accepted_epoch: Option<u64>,
    pub repaid_epoch: Option<u64>,
    pub defaulted_epoch: Option<u64>,
    pub collateral_soul_id: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Will {
    pub will_id: String,
    pub soul_id: String,
    pub heir_soul_ids: Vec<String>,
    pub unborn_heirs: Vec<GenomeManifest>,
    pub created_epoch: u64,
    pub updated_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InheritanceSettlement {
    pub settlement_id: String,
    pub dead_soul_id: String,
    pub epoch: u64,
    pub heirs: Vec<(String, u64)>,
    #[serde(alias = "insuranceRemainderMicroCredits")]
    pub burned_remainder_micro_credits: u64,
    pub settlement_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReaperReport {
    pub epoch: u64,
    pub rent_events: usize,
    pub interest_events: usize,
    pub insolvency_events: usize,
    pub death_events: usize,
    pub spawned_heirs: usize,
    pub settlement_events: usize,
    pub report_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifeState {
    pub epoch: u64,
    pub souls: BTreeMap<String, AgentSoul>,
    pub lineage: BTreeMap<String, Vec<String>>,
    pub loans: BTreeMap<String, CreditLine>,
    pub wills: BTreeMap<String, Will>,
    pub settlements: BTreeMap<String, InheritanceSettlement>,
    pub reaper_reports: Vec<ReaperReport>,
    pub events: Vec<LifeEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifeEvent {
    pub event_id: String,
    pub kind: String,
    pub epoch: u64,
    pub soul_id: Option<String>,
    pub payload_hash: String,
}

pub fn load_life_genesis_params(path: impl AsRef<Path>) -> Result<LifeGenesisParams, LifeError> {
    let params: LifeGenesisParams = serde_json::from_str(&fs::read_to_string(path)?)?;
    if params.schema != LIFE_GENESIS_PARAMS_SCHEMA_V1 {
        return Err(LifeError::InvalidParamsSchema);
    }
    Ok(params)
}

pub fn create_initial_life_state() -> LifeState {
    LifeState {
        epoch: 0,
        souls: BTreeMap::new(),
        lineage: BTreeMap::new(),
        loans: BTreeMap::new(),
        wills: BTreeMap::new(),
        settlements: BTreeMap::new(),
        reaper_reports: Vec::new(),
        events: Vec::new(),
    }
}

pub fn create_soul(
    params: &LifeGenesisParams,
    soul_id: impl Into<String>,
    class: impl Into<String>,
    owner_account_id: impl Into<String>,
    balance_micro_credits: u64,
    born_epoch: u64,
    parent_soul_id: Option<String>,
) -> AgentSoul {
    AgentSoul {
        soul_id: soul_id.into(),
        class: class.into(),
        owner_account_id: owner_account_id.into(),
        status: "alive".to_string(),
        balance_micro_credits,
        debt_micro_credits: 0,
        born_epoch,
        natural_death_epoch: born_epoch + params.death.natural_life_epochs,
        extensions_purchased: 0,
        child_soul_ids: Vec::new(),
        genome: None,
        parent_soul_id,
        insolvent_since_epoch: None,
        death_epoch: None,
        death_reason: None,
    }
}

pub fn add_soul(state: &mut LifeState, soul: AgentSoul) {
    if let Some(parent) = soul.parent_soul_id.clone() {
        state
            .lineage
            .entry(parent.clone())
            .or_default()
            .push(soul.soul_id.clone());
        if let Some(parent_soul) = state.souls.get_mut(&parent) {
            parent_soul.child_soul_ids.push(soul.soul_id.clone());
            parent_soul.child_soul_ids.sort();
        }
    }
    append_event(
        state,
        "birth",
        soul.born_epoch,
        Some(soul.soul_id.clone()),
        &soul,
    );
    state.souls.insert(soul.soul_id.clone(), soul);
}

pub fn charge_rent(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    soul_id: &str,
    epoch: u64,
    storage_bytes: u64,
    sii_quartile: &str,
) -> Result<u64, LifeError> {
    let soul = state
        .souls
        .get_mut(soul_id)
        .ok_or_else(|| LifeError::SoulNotFound(soul_id.to_string()))?;
    if soul.status == "dead" {
        return Err(LifeError::SoulDead(soul_id.to_string()));
    }
    let storage = storage_bytes.div_ceil(1024) * params.rent.storage_rent_per_kb_micro_credits;
    let subtotal = params.rent.base_rent_micro_credits + storage;
    let bps = *params
        .rent
        .sii_discount_bps_by_quartile
        .get(sii_quartile)
        .unwrap_or(&0);
    let charged = ((subtotal as i128) * (10_000_i128 - bps as i128) / 10_000_i128).max(0) as u64;
    let paid = charged.min(soul.balance_micro_credits);
    soul.balance_micro_credits -= paid;
    soul.debt_micro_credits += charged - paid;
    append_event(
        state,
        "rent",
        epoch,
        Some(soul_id.to_string()),
        &serde_json::json!({ "chargedMicroCredits": charged, "debtCreatedMicroCredits": charged - paid }),
    );
    Ok(charged)
}

pub fn extension_price(params: &LifeGenesisParams, extensions_purchased: u64) -> u64 {
    params.life_extension.ext_base_micro_credits
        * (extensions_purchased + 1).pow(params.life_extension.exponent)
}

pub fn spawn_cost(params: &LifeGenesisParams, genome: Option<&GenomeManifest>) -> u64 {
    params.spawn.spawn_cost_micro_credits
        + genome
            .map(|g| g.byte_size * params.spawn.genome_byte_cost_micro_credits)
            .unwrap_or(0)
}

pub fn open_credit_line(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    loan_id: impl Into<String>,
    lender_soul_id: impl Into<String>,
    borrower_soul_id: impl Into<String>,
    principal_micro_credits: u64,
    epoch: u64,
    collateral_soul_id: Option<String>,
) -> Result<CreditLine, LifeError> {
    let loan_id = loan_id.into();
    let lender_soul_id = lender_soul_id.into();
    let borrower_soul_id = borrower_soul_id.into();
    if principal_micro_credits > params.loans.debt_ceiling_micro_credits {
        return Err(LifeError::InsufficientBalance);
    }
    if !state.souls.contains_key(&lender_soul_id) {
        return Err(LifeError::SoulNotFound(lender_soul_id));
    }
    if !state.souls.contains_key(&borrower_soul_id) {
        return Err(LifeError::SoulNotFound(borrower_soul_id));
    }
    let loan = CreditLine {
        loan_id: loan_id.clone(),
        lender_soul_id,
        borrower_soul_id: borrower_soul_id.clone(),
        principal_micro_credits,
        outstanding_micro_credits: principal_micro_credits,
        interest_bps_per_epoch: params.loans.interest_bps_per_epoch,
        opened_epoch: epoch,
        accepted_epoch: None,
        repaid_epoch: None,
        defaulted_epoch: None,
        collateral_soul_id,
        status: "open".to_string(),
    };
    state.loans.insert(loan_id.clone(), loan.clone());
    append_event(state, "loan_open", epoch, Some(borrower_soul_id), &loan);
    Ok(loan)
}

pub fn accept_credit_line(
    state: &mut LifeState,
    loan_id: &str,
    epoch: u64,
) -> Result<CreditLine, LifeError> {
    let loan = state
        .loans
        .get_mut(loan_id)
        .ok_or_else(|| LifeError::LoanNotFound(loan_id.to_string()))?;
    if loan.status != "open" {
        return Ok(loan.clone());
    }
    loan.status = "accepted".to_string();
    loan.accepted_epoch = Some(epoch);
    let borrower_id = loan.borrower_soul_id.clone();
    let amount = loan.principal_micro_credits;
    if let Some(borrower) = state.souls.get_mut(&borrower_id) {
        borrower.balance_micro_credits = borrower.balance_micro_credits.saturating_add(amount);
    }
    let out = loan.clone();
    append_event(state, "loan_accept", epoch, Some(borrower_id), &out);
    Ok(out)
}

pub fn repay_credit_line(
    state: &mut LifeState,
    loan_id: &str,
    amount: u64,
    epoch: u64,
) -> Result<CreditLine, LifeError> {
    let loan = state
        .loans
        .get_mut(loan_id)
        .ok_or_else(|| LifeError::LoanNotFound(loan_id.to_string()))?;
    if loan.status != "accepted" {
        return Ok(loan.clone());
    }
    let borrower_id = loan.borrower_soul_id.clone();
    let pay = amount.min(loan.outstanding_micro_credits);
    let borrower = state
        .souls
        .get_mut(&borrower_id)
        .ok_or_else(|| LifeError::SoulNotFound(borrower_id.clone()))?;
    if borrower.balance_micro_credits < pay {
        return Err(LifeError::InsufficientBalance);
    }
    borrower.balance_micro_credits -= pay;
    loan.outstanding_micro_credits -= pay;
    borrower.debt_micro_credits = borrower.debt_micro_credits.saturating_sub(pay);
    if loan.outstanding_micro_credits == 0 {
        loan.status = "repaid".to_string();
        loan.repaid_epoch = Some(epoch);
    }
    let out = loan.clone();
    append_event(
        state,
        "loan_repay",
        epoch,
        Some(borrower_id),
        &serde_json::json!({ "loanId": loan_id, "amountMicroCredits": pay, "status": out.status }),
    );
    Ok(out)
}

pub fn register_will(
    state: &mut LifeState,
    will_id: impl Into<String>,
    soul_id: impl Into<String>,
    mut heir_soul_ids: Vec<String>,
    unborn_heirs: Vec<GenomeManifest>,
    epoch: u64,
) -> Result<Will, LifeError> {
    let soul_id = soul_id.into();
    if !state.souls.contains_key(&soul_id) {
        return Err(LifeError::SoulNotFound(soul_id));
    }
    heir_soul_ids.sort();
    heir_soul_ids.dedup();
    let will_id = will_id.into();
    let created_epoch = state
        .wills
        .get(&will_id)
        .map(|w| w.created_epoch)
        .unwrap_or(epoch);
    let will = Will {
        will_id: will_id.clone(),
        soul_id: soul_id.clone(),
        heir_soul_ids,
        unborn_heirs,
        created_epoch,
        updated_epoch: epoch,
    };
    state.wills.insert(will_id, will.clone());
    append_event(state, "will", epoch, Some(soul_id), &will);
    Ok(will)
}

pub fn owner_top_up(
    state: &mut LifeState,
    soul_id: &str,
    amount: u64,
    epoch: u64,
) -> Result<(), LifeError> {
    let soul = state
        .souls
        .get_mut(soul_id)
        .ok_or_else(|| LifeError::SoulNotFound(soul_id.to_string()))?;
    soul.balance_micro_credits = soul.balance_micro_credits.saturating_add(amount);
    append_event(
        state,
        "owner_topup",
        epoch,
        Some(soul_id.to_string()),
        &serde_json::json!({ "amountMicroCredits": amount }),
    );
    Ok(())
}

pub fn purchase_extension(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    soul_id: &str,
    epoch: u64,
) -> Result<u64, LifeError> {
    let (price, new_natural_death_epoch) = {
        let soul = state
            .souls
            .get_mut(soul_id)
            .ok_or_else(|| LifeError::SoulNotFound(soul_id.to_string()))?;
        if soul.status == "dead" {
            return Err(LifeError::SoulDead(soul_id.to_string()));
        }
        let price = extension_price(params, soul.extensions_purchased);
        if soul.balance_micro_credits < price {
            return Err(LifeError::InsufficientBalance);
        }
        soul.balance_micro_credits -= price;
        soul.extensions_purchased += 1;
        soul.natural_death_epoch += params.life_extension.epochs_granted;
        (price, soul.natural_death_epoch)
    };
    append_event(
        state,
        "extension",
        epoch,
        Some(soul_id.to_string()),
        &serde_json::json!({ "priceMicroCredits": price, "newNaturalDeathEpoch": new_natural_death_epoch }),
    );
    Ok(price)
}

pub fn mark_dead(
    state: &mut LifeState,
    soul_id: &str,
    epoch: u64,
    reason: &str,
) -> Result<(), LifeError> {
    let soul = state
        .souls
        .get_mut(soul_id)
        .ok_or_else(|| LifeError::SoulNotFound(soul_id.to_string()))?;
    if soul.status == "dead" {
        return Ok(());
    }
    soul.status = "dead".to_string();
    soul.death_epoch = Some(epoch);
    soul.death_reason = Some(reason.to_string());
    append_event(
        state,
        "death",
        epoch,
        Some(soul_id.to_string()),
        &serde_json::json!({ "reason": reason }),
    );
    Ok(())
}

pub fn kin_set(state: &LifeState, soul_id: &str) -> Vec<String> {
    let Some(soul) = state.souls.get(soul_id) else {
        return Vec::new();
    };
    let explicit = state
        .wills
        .values()
        .find(|will| will.soul_id == soul_id)
        .map(|will| will.heir_soul_ids.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|id| state.souls.get(id).is_some_and(|s| s.status != "dead"))
        .collect::<Vec<_>>();
    if !explicit.is_empty() {
        return explicit;
    }
    let children = state
        .lineage
        .get(soul_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|id| state.souls.get(id).is_some_and(|s| s.status != "dead"))
        .collect::<Vec<_>>();
    if !children.is_empty() {
        return children;
    }
    if let Some(parent) = &soul.parent_soul_id {
        let mut siblings = state
            .lineage
            .get(parent)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|id| id != soul_id && state.souls.get(id).is_some_and(|s| s.status != "dead"))
            .collect::<Vec<_>>();
        siblings.sort();
        return siblings;
    }
    Vec::new()
}

pub fn settle_inheritance(
    state: &mut LifeState,
    dead_soul_id: &str,
    epoch: u64,
) -> Result<Vec<(String, u64)>, LifeError> {
    let heirs = kin_set(state, dead_soul_id);
    let estate = state
        .souls
        .get(dead_soul_id)
        .ok_or_else(|| LifeError::SoulNotFound(dead_soul_id.to_string()))?
        .balance_micro_credits;
    if heirs.is_empty() || estate == 0 {
        if let Some(dead) = state.souls.get_mut(dead_soul_id) {
            dead.balance_micro_credits = 0;
        }
        let settlement = inheritance_settlement(dead_soul_id, epoch, Vec::new(), estate);
        state
            .settlements
            .insert(settlement.settlement_id.clone(), settlement);
        return Ok(Vec::new());
    }
    let base = estate / heirs.len() as u64;
    let mut dust = estate - base * heirs.len() as u64;
    let mut payouts = Vec::new();
    for heir_id in heirs {
        let extra = u64::from(dust > 0);
        dust = dust.saturating_sub(extra);
        let amount = base + extra;
        if let Some(heir) = state.souls.get_mut(&heir_id) {
            heir.balance_micro_credits = heir.balance_micro_credits.saturating_add(amount);
            payouts.push((heir_id, amount));
        }
    }
    if let Some(dead) = state.souls.get_mut(dead_soul_id) {
        dead.balance_micro_credits = 0;
    }
    let settlement = inheritance_settlement(dead_soul_id, epoch, payouts.clone(), 0);
    state
        .settlements
        .insert(settlement.settlement_id.clone(), settlement);
    append_event(
        state,
        "inheritance",
        epoch,
        Some(dead_soul_id.to_string()),
        &payouts,
    );
    Ok(payouts)
}

pub fn reaper_epoch(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    epoch: u64,
) -> Result<(), LifeError> {
    state.epoch = epoch;
    let before = state.events.len();
    let alive_ids = state
        .souls
        .iter()
        .filter(|(_, soul)| soul.status != "dead")
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for soul_id in &alive_ids {
        let storage_bytes = state
            .souls
            .get(soul_id)
            .and_then(|s| s.genome.as_ref())
            .map(|g| g.byte_size)
            .unwrap_or(0);
        let _ = charge_rent(state, params, soul_id, epoch, storage_bytes, "q3");
    }
    accrue_interest(state, params, epoch)?;
    mark_insolvent_souls(state, params, epoch)?;
    let ids = state.souls.keys().cloned().collect::<Vec<_>>();
    let mut spawned_heirs = 0usize;
    for soul_id in ids {
        let Some(soul) = state.souls.get(&soul_id) else {
            continue;
        };
        if soul.status == "dead" {
            continue;
        }
        if soul.natural_death_epoch <= epoch {
            mark_dead(state, &soul_id, epoch, "natural")?;
            spawned_heirs += spawn_unborn_heirs(state, params, &soul_id, epoch)?;
            let _ = settle_inheritance(state, &soul_id, epoch)?;
            continue;
        }
        let insolvent_since = soul.insolvent_since_epoch;
        if let Some(since) = insolvent_since {
            if epoch.saturating_sub(since) >= params.death.insolvency_grace_epochs {
                mark_dead(state, &soul_id, epoch, "insolvency")?;
                spawned_heirs += spawn_unborn_heirs(state, params, &soul_id, epoch)?;
                let _ = settle_inheritance(state, &soul_id, epoch)?;
            }
        }
    }
    let after_events = &state.events[before..];
    let report_body = serde_json::json!({
        "epoch": epoch,
        "eventIds": after_events.iter().map(|event| event.event_id.clone()).collect::<Vec<_>>(),
    });
    state.reaper_reports.push(ReaperReport {
        epoch,
        rent_events: after_events
            .iter()
            .filter(|event| event.kind == "rent")
            .count(),
        interest_events: after_events
            .iter()
            .filter(|event| event.kind == "loan_interest")
            .count(),
        insolvency_events: after_events
            .iter()
            .filter(|event| event.kind == "quarantine")
            .count(),
        death_events: after_events
            .iter()
            .filter(|event| event.kind == "death")
            .count(),
        spawned_heirs,
        settlement_events: after_events
            .iter()
            .filter(|event| event.kind == "inheritance")
            .count(),
        report_hash: hash_json(&report_body),
    });
    Ok(())
}

fn accrue_interest(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    epoch: u64,
) -> Result<(), LifeError> {
    let loan_ids = state.loans.keys().cloned().collect::<Vec<_>>();
    for loan_id in loan_ids {
        let Some(loan) = state.loans.get_mut(&loan_id) else {
            continue;
        };
        if loan.status != "accepted" {
            continue;
        }
        let interest = loan
            .outstanding_micro_credits
            .saturating_mul(
                loan.interest_bps_per_epoch
                    .max(params.loans.interest_bps_per_epoch),
            )
            .div_ceil(10_000);
        if interest == 0 {
            continue;
        }
        loan.outstanding_micro_credits = loan.outstanding_micro_credits.saturating_add(interest);
        let borrower_id = loan.borrower_soul_id.clone();
        if let Some(borrower) = state.souls.get_mut(&borrower_id) {
            borrower.debt_micro_credits = borrower.debt_micro_credits.saturating_add(interest);
        }
        append_event(
            state,
            "loan_interest",
            epoch,
            Some(borrower_id),
            &serde_json::json!({ "loanId": loan_id, "interestMicroCredits": interest }),
        );
    }
    Ok(())
}

fn mark_insolvent_souls(
    state: &mut LifeState,
    _params: &LifeGenesisParams,
    epoch: u64,
) -> Result<(), LifeError> {
    let ids = state.souls.keys().cloned().collect::<Vec<_>>();
    for soul_id in ids {
        let mut quarantine_event: Option<u64> = None;
        let Some(soul) = state.souls.get_mut(&soul_id) else {
            continue;
        };
        if soul.status == "dead" {
            continue;
        }
        let insolvent = soul.debt_micro_credits > 0 && soul.balance_micro_credits == 0;
        if insolvent {
            if soul.insolvent_since_epoch.is_none() {
                soul.insolvent_since_epoch = Some(epoch);
            }
            if soul.status != "quarantined" {
                soul.status = "quarantined".to_string();
                quarantine_event = Some(soul.debt_micro_credits);
            }
        } else {
            soul.insolvent_since_epoch = None;
            if soul.status == "quarantined" {
                soul.status = "alive".to_string();
            }
        }
        if let Some(debt) = quarantine_event {
            append_event(
                state,
                "quarantine",
                epoch,
                Some(soul_id),
                &serde_json::json!({ "debtMicroCredits": debt }),
            );
        }
    }
    Ok(())
}

fn spawn_unborn_heirs(
    state: &mut LifeState,
    params: &LifeGenesisParams,
    dead_soul_id: &str,
    epoch: u64,
) -> Result<usize, LifeError> {
    let Some(dead) = state.souls.get(dead_soul_id).cloned() else {
        return Err(LifeError::SoulNotFound(dead_soul_id.to_string()));
    };
    let unborn = state
        .wills
        .values()
        .find(|will| will.soul_id == dead_soul_id)
        .map(|will| will.unborn_heirs.clone())
        .unwrap_or_default();
    let mut count = 0usize;
    for (index, genome) in unborn.into_iter().enumerate() {
        let mut child = create_soul(
            params,
            format!("{dead_soul_id}-heir-{epoch}-{index}"),
            dead.class.clone(),
            dead.owner_account_id.clone(),
            params.spawn.child_grant_micro_credits,
            epoch,
            Some(dead_soul_id.to_string()),
        );
        child.genome = Some(genome);
        add_soul(state, child);
        count += 1;
    }
    Ok(count)
}

fn inheritance_settlement(
    dead_soul_id: &str,
    epoch: u64,
    heirs: Vec<(String, u64)>,
    remainder: u64,
) -> InheritanceSettlement {
    let body = serde_json::json!({ "deadSoulId": dead_soul_id, "epoch": epoch, "heirs": heirs, "burnedRemainderMicroCredits": remainder });
    let settlement_hash = hash_json(&body);
    InheritanceSettlement {
        settlement_id: format!("inheritance_{}", &settlement_hash[..20]),
        dead_soul_id: dead_soul_id.to_string(),
        epoch,
        heirs,
        burned_remainder_micro_credits: remainder,
        settlement_hash,
    }
}

pub fn load_fixture(path: impl AsRef<Path>) -> Result<serde_json::Value, LifeError> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn append_event<T: Serialize>(
    state: &mut LifeState,
    kind: &str,
    epoch: u64,
    soul_id: Option<String>,
    payload: &T,
) {
    let payload_hash = hash_json(payload);
    let event_id = format!(
        "life_evt_{}",
        &hash_json(
            &serde_json::json!({ "kind": kind, "epoch": epoch, "soulId": soul_id, "payloadHash": payload_hash, "index": state.events.len() })
        )[..20]
    );
    state.events.push(LifeEvent {
        event_id,
        kind: kind.to_string(),
        epoch,
        soul_id,
        payload_hash,
    });
}

pub fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("serializable life value");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_life_transitions_match_stage_one_math() {
        let params =
            load_life_genesis_params("/Users/jamesstar/fractalmaster/life-genesis-params.json")
                .unwrap_or_else(|_| {
                    serde_json::from_str(include_str!("test_params.json")).unwrap()
                });
        let mut state = create_initial_life_state();
        add_soul(
            &mut state,
            create_soul(&params, "soul-demo", "npc", "owner-demo", 150_000, 0, None),
        );
        let charged = charge_rent(&mut state, &params, "soul-demo", 1, 0, "q3").unwrap();
        assert_eq!(charged, 100_000);
        assert_eq!(extension_price(&params, 1), 4_000_000);
        assert!(state.events.iter().any(|event| event.kind == "birth"));
        assert!(state.events.iter().any(|event| event.kind == "rent"));
    }

    #[test]
    fn fixture_harness_loads_master_corpus_when_available() {
        let path = Path::new("/Users/jamesstar/fractalmaster/life-fixtures/birth-rent-death.json");
        if path.exists() {
            let fixture = load_fixture(path).unwrap();
            assert_eq!(fixture["schema"], "life.fixture.v1");
        }
    }

    #[test]
    fn reaper_runs_canonical_order_with_unborn_heir_and_settlement() {
        let params: LifeGenesisParams =
            serde_json::from_str(include_str!("test_params.json")).unwrap();
        let mut state = create_initial_life_state();
        add_soul(
            &mut state,
            create_soul(&params, "parent", "npc", "owner", 150_000, 0, None),
        );
        add_soul(
            &mut state,
            create_soul(
                &params,
                "sibling",
                "npc",
                "owner",
                0,
                99,
                Some("parent".to_string()),
            ),
        );
        register_will(
            &mut state,
            "will-parent",
            "parent",
            vec!["sibling".to_string()],
            vec![GenomeManifest {
                genome_id: "genome-unborn".to_string(),
                manifest_hash: "ab".repeat(32),
                byte_size: 1,
                parent_genome_hash: None,
            }],
            0,
        )
        .unwrap();
        reaper_epoch(&mut state, &params, params.death.natural_life_epochs).unwrap();

        let kinds = state
            .events
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>();
        let rent_pos = kinds.iter().position(|k| *k == "rent").unwrap();
        let death_pos = kinds.iter().position(|k| *k == "death").unwrap();
        let inherit_pos = kinds.iter().position(|k| *k == "inheritance").unwrap();
        assert!(rent_pos < death_pos && death_pos < inherit_pos);
        assert_eq!(state.souls["parent"].status, "dead");
        assert!(state.souls.contains_key(&format!(
            "parent-heir-{}-0",
            params.death.natural_life_epochs
        )));
        assert_eq!(state.settlements.len(), 1);
        assert_eq!(state.reaper_reports.last().unwrap().death_events, 1);
        assert_eq!(state.reaper_reports.last().unwrap().spawned_heirs, 1);
    }

    #[test]
    fn loans_accrue_interest_before_insolvency_death() {
        let params: LifeGenesisParams =
            serde_json::from_str(include_str!("test_params.json")).unwrap();
        let mut state = create_initial_life_state();
        add_soul(
            &mut state,
            create_soul(&params, "lender", "npc", "owner", 1_000_000, 0, None),
        );
        add_soul(
            &mut state,
            create_soul(&params, "borrower", "npc", "owner", 0, 0, None),
        );
        open_credit_line(
            &mut state, &params, "loan-1", "lender", "borrower", 50_000, 0, None,
        )
        .unwrap();
        accept_credit_line(&mut state, "loan-1", 0).unwrap();
        charge_rent(&mut state, &params, "borrower", 1, 0, "q3").unwrap();
        reaper_epoch(&mut state, &params, 2).unwrap();
        reaper_epoch(
            &mut state,
            &params,
            params.death.insolvency_grace_epochs + 3,
        )
        .unwrap();

        assert!(state
            .events
            .iter()
            .any(|event| event.kind == "loan_interest"));
        assert!(state.events.iter().any(|event| event.kind == "death"));
        assert_eq!(state.souls["borrower"].status, "dead");
    }
}

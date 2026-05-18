//! On-chain wallet operator helpers (`docs/wallet.md` §14.1–14.2).

use borsh::BorshDeserialize;
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use fractal_core::{
    apply_block, Account, Address, NativeCall, State, Transaction, TxBody, VmKind,
    WalletBudgetSeed, WalletRevokeCapabilitySignBody, WalletToolBatchSettlePayload,
    HARDHAT_DEFAULT_SIGNER_0,
};
use fractal_wallet::{
    prepare_wallet_batch_receipts, sign_wallet_tool_batch, CapabilityToken, RevocationEntry,
    RevocationSet, ToolClass, ToolReceipt,
};
use serde_json::{json, Value};

use crate::{parse_cap_id, parse_hex, parse_flags, CapFlags};

fn parse_address(s: &str, label: &str) -> Result<Address, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(h).map_err(|e| format!("{label} hex: {e}"))?;
    if bytes.len() != 20 {
        return Err(format!("{label} must be 20 bytes"));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn resolve_signer(parsed: &CapFlags) -> Result<Address, String> {
    match &parsed.signer_hex {
        Some(s) => parse_address(s, "--signer"),
        None => Ok(HARDHAT_DEFAULT_SIGNER_0),
    }
}

pub(crate) fn resolve_rpc_url(parsed: &CapFlags) -> Option<String> {
    parsed
        .rpc_url
        .clone()
        .or_else(|| std::env::var("FRACTAL_RPC_URL").ok())
}

/// Load on-chain revocation rows + root from JSON-RPC (`docs/wallet.md` §4.6).
pub(crate) fn load_revocation_set_from_rpc(
    rpc_url: &str,
) -> Result<(RevocationSet, [u8; 32]), String> {
    let root_v = rpc_post(
        rpc_url,
        "fractal_getWalletRevocationMerkleRoot",
        json!([]),
    )?;
    let root_hex = root_v.as_str().ok_or("revocation root not string")?;
    let root = parse_cap_id(root_hex)?;

    let entries_v = rpc_post(
        rpc_url,
        "fractal_getWalletRevocationEntries",
        json!([]),
    )?;
    let entries = entries_v
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("missing entries array")?;

    let mut set = RevocationSet::default();
    for row in entries {
        let cap_hex = row
            .get("capId")
            .and_then(Value::as_str)
            .ok_or("entry missing capId")?;
        let cap_id = parse_cap_id(cap_hex)?;
        let revoked_at_ms = row
            .get("revokedAtMs")
            .and_then(Value::as_u64)
            .ok_or("entry missing revokedAtMs")?;
        let reason_code = row
            .get("reasonCode")
            .and_then(Value::as_u64)
            .ok_or("entry missing reasonCode")? as u8;
        let cascade = row
            .get("cascade")
            .and_then(Value::as_bool)
            .ok_or("entry missing cascade")?;
        set.revoke(
            cap_id,
            RevocationEntry {
                revoked_at_ms,
                reason_code,
                cascade,
            },
        )
        .map_err(|e| format!("revoke {cap_hex}: {e}"))?;
    }

    if set.root() != root {
        return Err(format!(
            "rpc root 0x{} != entries-derived root 0x{}",
            hex::encode(root),
            hex::encode(set.root())
        ));
    }
    Ok((set, root))
}

pub(crate) fn rpc_post(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp: Value = ureq::post(url)
        .set("Content-Type", "application/json; charset=utf-8")
        .send_json(body)
        .map_err(|e| format!("http: {e}"))?
        .into_json()
        .map_err(|e| format!("json: {e}"))?;
    if let Some(err) = resp.get("error") {
        return Err(format!("rpc error: {err}"));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| "missing result".to_string())
}

pub(crate) fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn fetch_nonce(rpc_url: &str, signer: &Address) -> Result<u64, String> {
    let v = rpc_post(
        rpc_url,
        "eth_getTransactionCount",
        json!([addr_hex(signer), "latest"]),
    )?;
    let s = v.as_str().ok_or("nonce not string")?;
    let h = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(h, 16).map_err(|e| format!("parse nonce: {e}"))
}

fn send_borsh_tx(rpc_url: &str, tx: &Transaction) -> Result<String, String> {
    let raw = borsh::to_vec(tx).map_err(|e| format!("borsh encode tx: {e}"))?;
    let hex_raw = format!("0x{}", hex::encode(raw));
    let h = rpc_post(rpc_url, "eth_sendRawTransaction", json!([hex_raw]))?;
    h.as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "tx hash not string".to_string())
}

fn funded_local_state(signer: Address, balance: u128, execution_timestamp_ms: u64) -> State {
    let mut state = State::default();
    state.execution_timestamp_ms = execution_timestamp_ms;
    state.accounts.insert(
        signer,
        Account {
            nonce: 0,
            balance,
        },
    );
    state
}

fn submit_native(
    parsed: &CapFlags,
    call: NativeCall,
    local_fields: impl FnOnce(&State) -> Value,
) -> Result<Value, String> {
    let signer = resolve_signer(parsed)?;
    let execution_ts = parsed.not_after_ms.unwrap_or(1_000_000);

    if parsed.apply_local {
        let nonce = parsed.nonce.unwrap_or(0);
        let tx = Transaction {
            signer,
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(call),
        };
        let mut state = funded_local_state(signer, u128::MAX / 4, execution_ts);
        apply_block(&mut state, &[tx]).map_err(|e| format!("apply_block: {e:?}"))?;
        let mut out = json!({
            "mode": "apply-local",
            "signer": addr_hex(&signer),
            "nonce": nonce,
        });
        let extra = local_fields(&state);
        if let (Some(base), Some(more)) = (out.as_object_mut(), extra.as_object()) {
            for (k, v) in more {
                base.insert(k.clone(), v.clone());
            }
        }
        return Ok(out);
    }

    let rpc_url = resolve_rpc_url(parsed).ok_or(
        "submit requires --rpc-url, FRACTAL_RPC_URL, or --apply-local for dev in-process apply",
    )?;
    let nonce = match parsed.nonce {
        Some(n) => n,
        None => fetch_nonce(&rpc_url, &signer)?,
    };
    let tx = Transaction {
        signer,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    };
    let tx_hash = send_borsh_tx(&rpc_url, &tx)?;
    Ok(json!({
        "mode": "rpc",
        "rpcUrl": rpc_url,
        "signer": addr_hex(&signer),
        "nonce": nonce,
        "txHash": tx_hash,
        "rawTxHex": format!("0x{}", hex::encode(borsh::to_vec(&tx).map_err(|e| format!("borsh: {e}"))?)),
    }))
}

fn build_mint_revocation_proof_borsh(
    parsed: &CapFlags,
    token: &CapabilityToken,
    parent_cap_id: Option<[u8; 32]>,
) -> Result<Vec<u8>, String> {
    let mut set = RevocationSet::default();
    if parsed.from_rpc {
        let rpc = resolve_rpc_url(parsed)
            .ok_or("--from-rpc requires --rpc-url or FRACTAL_RPC_URL")?;
        let (loaded, _) = load_revocation_set_from_rpc(&rpc)?;
        set = loaded;
    } else if parsed.apply_local {
        // Fresh apply-local state has no revocations unless caller used prior txs in same process.
    } else if let Some(rpc) = resolve_rpc_url(parsed) {
        let (loaded, _) = load_revocation_set_from_rpc(&rpc)?;
        set = loaded;
    } else {
        return Err(
            "submit-mint-cap requires --proof-hex or --from-rpc / FRACTAL_RPC_URL to build proof"
                .into(),
        );
    }
    let mut ancestors = Vec::new();
    if let Some(p) = parent_cap_id.or(token.body.parent_cap_id) {
        ancestors.push(p);
    }
    let proof = set
        .build_verify_proof(token.body.cap_id, &ancestors)
        .map_err(|e| format!("build revocation proof: {e}"))?;
    borsh::to_vec(&proof).map_err(|e| format!("borsh proof: {e}"))
}

pub fn cmd_chain_submit_mint_cap(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let token_hex = parsed
        .token_hex
        .as_ref()
        .ok_or("--token-hex required (borsh CapabilityToken)")?;
    let token_bytes = parse_hex(token_hex, "--token-hex")?;
    let probe: CapabilityToken =
        CapabilityToken::try_from_slice(&token_bytes).map_err(|e| format!("token borsh: {e}"))?;

    let parent_cap_id = match &parsed.parent_cap_id_hex {
        Some(h) => Some(parse_cap_id(h)?),
        None => None,
    };
    let budget_seed = match (parsed.from_budget, parsed.seed_amount) {
        (None, None) => None,
        (Some(from), Some(amount)) => Some(WalletBudgetSeed {
            from_budget: from,
            amount,
        }),
        _ => {
            return Err(
                "budget seed requires both --from-budget and --seed-amount (or omit both)"
                    .into(),
            );
        }
    };

    let cap_id = probe.body.cap_id;
    let revocation_proof_borsh = if let Some(hex) = &parsed.proof_hex {
        parse_hex(hex, "--proof-hex")?
    } else {
        build_mint_revocation_proof_borsh(&parsed, &probe, parent_cap_id)?
    };
    let call = NativeCall::WalletMintCapabilityV1 {
        parent_cap_id,
        child_token_borsh: token_bytes,
        budget_seed: budget_seed.clone(),
        revocation_proof_borsh,
    };

    submit_native(&parsed, call, |state| {
        json!({
            "capId": format!("0x{}", hex::encode(cap_id)),
            "registered": state.wallet_capabilities.contains_key(&cap_id),
            "holder": state.wallet_cap_holders.get(&cap_id).map(addr_hex),
            "budgetSeed": budget_seed.map(|s| json!({
                "fromBudget": s.from_budget,
                "amount": s.amount.to_string(),
            })),
        })
    })
}

pub fn cmd_chain_submit_create_budget(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let initial_deposit = parsed
        .initial_deposit
        .or(parsed.parent_budget_deposited)
        .ok_or("--initial-deposit required")?;
    let parent = parsed.budget_parent;

    let call = NativeCall::WalletCreateBudgetAccountV1 {
        parent,
        initial_deposit,
    };

    submit_native(&parsed, call, |state| {
        let budget_id = state.next_wallet_budget_id.saturating_sub(1);
        let row = state.wallet_budgets.get(&budget_id);
        json!({
            "budgetId": budget_id,
            "parent": row.and_then(|r| r.parent),
            "totalDeposited": row.map(|r| r.total_deposited.to_string()),
            "owner": row.map(|r| addr_hex(&r.owner)),
        })
    })
}

fn sign_revoke_capability(
    issuer_secret_hex: &str,
    cap_id: [u8; 32],
    reason_code: u8,
    cascade: bool,
    chain_id: u32,
) -> Result<[u8; 64], String> {
    use ed25519_dalek::SigningKey;
    let secret = crate::parse_hex32(issuer_secret_hex, "--issuer-secret")?;
    let sk = SigningKey::from_bytes(&secret);
    let body = WalletRevokeCapabilitySignBody {
        cap_id,
        reason_code,
        cascade,
        chain_id,
    };
    let msg = borsh::to_vec(&body).map_err(|e| format!("revoke sign bytes: {e}"))?;
    Ok(sk.sign(&msg).to_bytes())
}

pub fn cmd_chain_submit_revoke_cap(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let cap_id = parse_cap_id(
        parsed
            .cap_id_hex
            .as_ref()
            .ok_or("--cap-id required")?,
    )?;
    let reason_code = parsed.reason_code.unwrap_or(0);
    let cascade = parsed.revoke_cascade;
    let chain_id = parsed.chain_id.unwrap_or(41);

    let issuer_sig = if let Some(sig_hex) = &parsed.issuer_sig_hex {
        let bytes = parse_hex(sig_hex, "--issuer-sig")?;
        if bytes.len() != 64 {
            return Err("--issuer-sig must be 64 bytes".into());
        }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&bytes);
        sig
    } else {
        let secret = parsed
            .issuer_secret_hex
            .as_ref()
            .ok_or("--issuer-secret or --issuer-sig required")?;
        sign_revoke_capability(secret, cap_id, reason_code, cascade, chain_id)?
    };

    let call = NativeCall::WalletRevokeCapabilityV1 {
        cap_id,
        reason_code,
        cascade,
        issuer_sig,
    };

    submit_native(&parsed, call, move |state| {
        let entry = state.wallet_revocation_entries.get(&cap_id);
        json!({
            "capId": format!("0x{}", hex::encode(cap_id)),
            "revoked": entry.is_some(),
            "cascade": entry.map(|e| e.cascade),
            "reasonCode": entry.map(|e| e.reason_code),
            "revokedAtMs": entry.map(|e| e.revoked_at_ms),
        })
    })
}

pub fn cmd_chain_submit_fund_budget(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let budget = parsed
        .budget_id
        .or(parsed.child_budget_id)
        .ok_or("--budget required")?;
    let amount = parsed
        .fund_amount
        .or(parsed.delegate_amount)
        .or(parsed.seed_amount)
        .ok_or("--amount required")?;

    let source_budget = parsed.source_budget;
    let call = NativeCall::WalletFundBudgetAccountV1 {
        budget,
        amount,
        source_budget,
    };

    submit_native(&parsed, call, move |state| {
        let row = state.wallet_budgets.get(&budget);
        json!({
            "budgetId": budget,
            "totalDeposited": row.map(|r| r.total_deposited.to_string()),
            "available": row.map(|r| r.available().to_string()),
            "sourceBudget": source_budget,
        })
    })
}

pub fn cmd_chain_submit_reputation_snapshot(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let provider_hex = parsed
        .provider_id_hex
        .as_ref()
        .ok_or("--provider-id required (32-byte hex)")?;
    let provider_id = parse_cap_id(provider_hex)?;
    let tool_class = parsed
        .tool_class
        .ok_or("--tool-class required (u8 discriminant)")?;

    let summary_borsh = if let Some(hx) = &parsed.summary_borsh_hex {
        parse_hex(hx, "--summary-borsh-hex")?
    } else if let Some(path) = &parsed.summary_json_path {
        let summary = crate::reputation::load_summary_from_path(path)?;
        if summary.tool_class as u8 != tool_class {
            return Err(format!(
                "--tool-class {tool_class} != summary.tool_class {}",
                summary.tool_class as u8
            ));
        }
        borsh::to_vec(&summary).map_err(|e| format!("borsh summary: {e}"))?
    } else {
        return Err("provide --summary-borsh-hex or --summary-json".into());
    };

    let call = NativeCall::WalletReputationSnapshotV1 {
        provider_id,
        tool_class,
        summary_borsh,
    };

    submit_native(
        &parsed,
        call,
        |state| {
            let score = state.wallet_reputation_score_milli(&provider_id, tool_class);
            let commitment = state
                .wallet_reputation_ledger_commitment
                .get(&(provider_id, tool_class))
                .copied();
            json!({
                "providerId": format!("0x{}", hex::encode(provider_id)),
                "toolClass": tool_class,
                "scoreMilli": score.map(|s| s.to_string()),
                "ledgerCommitment": commitment.map(|c| format!("0x{}", hex::encode(c))),
            })
        },
    )
}

pub fn cmd_chain_submit_wallet_batch_settle(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let batch_id = parse_cap_id(
        parsed
            .batch_id_hex
            .as_ref()
            .ok_or("--batch-id required (32-byte hex)")?,
    )?;
    let payout_to = parse_address(
        parsed
            .payout_to_hex
            .as_ref()
            .ok_or("--payout-to required (20-byte hex address)")?,
        "--payout-to",
    )?;
    let tool_class = parsed
        .tool_class
        .ok_or("--tool-class required (u8 discriminant)")?;
    let provider_secret_hex = parsed
        .provider_secret_hex
        .as_ref()
        .ok_or("--provider-secret required (32-byte hex)")?;
    let provider_secret = parse_hex(provider_secret_hex, "--provider-secret")?;
    if provider_secret.len() != 32 {
        return Err("--provider-secret must be 32 bytes".into());
    }
    let mut sk_bytes = [0u8; 32];
    sk_bytes.copy_from_slice(&provider_secret);
    let provider_sk = SigningKey::from_bytes(&sk_bytes);
    let provider_pk = provider_sk.verifying_key().to_bytes();

    if parsed.receipt_borsh_hex.is_empty() {
        return Err("at least one --receipt-borsh-hex required".into());
    }
    let mut receipts = Vec::with_capacity(parsed.receipt_borsh_hex.len());
    for (i, hx) in parsed.receipt_borsh_hex.iter().enumerate() {
        let blob = parse_hex(hx, "--receipt-borsh-hex")?;
        let r: ToolReceipt = borsh::from_slice(&blob)
            .map_err(|e| format!("receipt {i} borsh decode: {e}"))?;
        receipts.push(r);
    }
    if receipts
        .iter()
        .any(|r| r.body.tool_class as u8 != tool_class)
    {
        return Err("--tool-class does not match receipt tool_class".into());
    }
    let tool_class_enum = receipts[0].body.tool_class;
    let total_cost = match parsed.total_cost {
        Some(t) => t,
        None => receipts.iter().map(|r| r.body.cost).sum(),
    };
    let provider_id = fractal_wallet::provider_id_from_public_key(&provider_pk);
    let (receipt_root, receipts_borsh) = prepare_wallet_batch_receipts(
        &receipts,
        provider_id,
        tool_class_enum,
        total_cost,
    )
    .map_err(|e| format!("prepare batch: {e}"))?;
    let receipt_count = u32::try_from(receipts.len())
        .map_err(|_| "too many receipts".to_string())?;
    let (_, provider_batch_sig) = sign_wallet_tool_batch(
        &provider_sk,
        batch_id,
        receipt_root,
        total_cost,
        receipt_count,
        payout_to,
    )
    .map_err(|e| format!("sign batch: {e}"))?;
    let submitted_at = parsed.submitted_at_ms.unwrap_or(0);

    let payload = WalletToolBatchSettlePayload {
        batch_id,
        provider_id,
        provider_public_key: provider_pk,
        tool_class,
        receipt_root,
        total_cost,
        payout_to,
        receipts_borsh,
        submitted_at,
        provider_batch_sig,
    };
    let call = NativeCall::WalletBatchSettleV1(payload.clone());

    submit_native(&parsed, call, |state| {
        let stored = state.wallet_tool_batches.get(&batch_id);
        json!({
            "batchId": format!("0x{}", hex::encode(batch_id)),
            "receiptRoot": format!("0x{}", hex::encode(receipt_root)),
            "totalCost": total_cost.to_string(),
            "receiptCount": receipt_count,
            "stored": stored.is_some(),
        })
    })
}

pub fn cmd_chain_emergency_stop(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let engage = parsed
        .emergency_engage
        .ok_or("chain emergency-stop requires --engage or --disengage")?;
    let call = NativeCall::WalletEmergencyStopV1 { engage };
    submit_native(&parsed, call, |state| {
        json!({
            "engaged": state.wallet_emergency_stop,
        })
    })
}

fn parse_sig64(s: &str, label: &str) -> Result<[u8; 64], String> {
    let b = parse_hex(s, label)?;
    if b.len() != 64 {
        return Err(format!("{label} must be 64 bytes"));
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&b);
    Ok(out)
}

pub fn cmd_chain_task_post(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let metadata_uri = parsed.metadata_uri.clone().unwrap_or_default();
    let bounty_budget = parsed.bounty_budget.ok_or("--bounty-budget required")?;
    let tool_budget = parsed.tool_budget.unwrap_or(0);
    let verifier_budget = parsed.verifier_budget.unwrap_or(0);
    let call = NativeCall::WalletPostTaskV1 {
        metadata_uri,
        bounty_budget,
        tool_budget,
        verifier_budget,
    };
    submit_native(&parsed, call, |state| {
        let id = state.next_wallet_task_id.saturating_sub(1);
        let row = state.wallet_tasks.get(&id);
        json!({
            "taskId": id,
            "status": row.map(|r| r.status),
            "escrowWei": row.map(|r| r.escrow_wei.to_string()),
            "owner": row.map(|r| addr_hex(&r.owner)),
        })
    })
}

pub fn cmd_chain_task_checkout(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let task_id = parsed.task_id.ok_or("--task-id required")?;
    let agent_session = parse_cap_id(
        parsed
            .agent_session_hex
            .as_ref()
            .ok_or("--agent-session-hex required (32-byte hex)")?,
    )?;
    let expiry_ms = parsed.expiry_ms.ok_or("--expiry-ms required")?;
    let call = NativeCall::WalletCheckoutTaskV1 {
        task_id,
        agent_session,
        expiry_ms,
    };
    submit_native(&parsed, call, move |state| {
        let row = state.wallet_tasks.get(&task_id);
        json!({
            "taskId": task_id,
            "status": row.map(|r| r.status),
            "checkoutExpiryMs": row.map(|r| r.checkout_expiry_ms),
            "checkoutSigner": row.and_then(|r| r.checkout_signer.map(|a| addr_hex(&a))),
        })
    })
}

pub fn cmd_chain_task_renew_checkout(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let task_id = parsed.task_id.ok_or("--task-id required")?;
    let evidence_uri = parsed
        .evidence_uri
        .clone()
        .unwrap_or_default();
    let new_expiry_ms = parsed.new_expiry_ms.ok_or("--new-expiry-ms required")?;
    let call = NativeCall::WalletRenewCheckoutV1 {
        task_id,
        evidence_uri: evidence_uri.clone(),
        new_expiry_ms,
    };
    submit_native(&parsed, call, move |state| {
        let row = state.wallet_tasks.get(&task_id);
        json!({
            "taskId": task_id,
            "status": row.map(|r| r.status),
            "checkoutExpiryMs": row.map(|r| r.checkout_expiry_ms),
            "evidenceUri": row.map(|r| r.renew_evidence_uri.clone()),
        })
    })
}

pub fn cmd_chain_task_submit(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let task_id = parsed.task_id.ok_or("--task-id required")?;
    let artifact_pointer = parsed
        .artifact_pointer
        .clone()
        .ok_or("--artifact-pointer required")?;
    let tool_receipt_root = parse_cap_id(
        parsed
            .tool_receipt_root_hex
            .as_ref()
            .ok_or("--tool-receipt-root required (32-byte hex)")?,
    )?;
    let call = NativeCall::WalletSubmitTaskV1 {
        task_id,
        artifact_pointer: artifact_pointer.clone(),
        tool_receipt_root,
    };
    submit_native(&parsed, call, move |state| {
        let row = state.wallet_tasks.get(&task_id);
        json!({
            "taskId": task_id,
            "status": row.map(|r| r.status),
            "artifactPointer": row.map(|r| r.artifact_pointer.clone()),
            "toolReceiptRoot": row.map(|r| format!("0x{}", hex::encode(r.tool_receipt_root))),
        })
    })
}

pub fn cmd_chain_task_verify(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let task_id = parsed.task_id.ok_or("--task-id required")?;
    let verifier_sig = parse_sig64(
        parsed
            .verifier_sig_hex
            .as_ref()
            .ok_or("--verifier-sig required (64-byte hex)")?,
        "--verifier-sig",
    )?;
    let score = parsed.verify_score.unwrap_or(0);
    let call = NativeCall::WalletVerifyTaskV1 {
        task_id,
        verifier_sig,
        score,
    };
    submit_native(&parsed, call, move |state| {
        let row = state.wallet_tasks.get(&task_id);
        json!({
            "taskId": task_id,
            "status": row.map(|r| r.status),
            "verifierScore": row.map(|r| r.verifier_score),
        })
    })
}

pub fn cmd_chain_task_finalize(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let task_id = parsed.task_id.ok_or("--task-id required")?;
    let call = NativeCall::WalletFinalizeTaskV1 { task_id };
    submit_native(&parsed, call, move |state| {
        let row = state.wallet_tasks.get(&task_id);
        json!({
            "taskId": task_id,
            "status": row.map(|r| r.status),
            "escrowWei": row.map(|r| r.escrow_wei.to_string()),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use fractal_core::WEI_PER_FRAC;
    use fractal_wallet::{
        capability::{CapabilitySignBody, CapabilityToken},
        caveat::Caveat,
        policy::builtins::FRAC,
        types::{Scope, ToolClass},
    };
    use rand::rngs::OsRng;

    #[test]
    fn emergency_stop_apply_local_toggles_state() {
        let v_on = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "emergency-stop".into(),
            "--engage".into(),
            "--apply-local".into(),
        ])
        .unwrap();
        assert_eq!(v_on.get("engaged").and_then(Value::as_bool), Some(true));

        let v_off = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "emergency-stop".into(),
            "--disengage".into(),
            "--apply-local".into(),
        ])
        .unwrap();
        assert_eq!(v_off.get("engaged").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn task_post_apply_local_returns_task_id() {
        let v = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "task-post".into(),
            "--metadata-uri".into(),
            "ipfs://test".into(),
            "--bounty-budget".into(),
            "100".into(),
            "--tool-budget".into(),
            "0".into(),
            "--verifier-budget".into(),
            "0".into(),
            "--apply-local".into(),
        ])
        .unwrap();
        assert_eq!(v.get("taskId").and_then(serde_json::Value::as_u64), Some(1));
        assert_eq!(v.get("status").and_then(serde_json::Value::as_u64), Some(0));
    }

    #[test]
    fn submit_mint_cap_apply_local_registers_capability() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let subject = SigningKey::generate(&mut rng);
        let body = CapabilitySignBody {
            version: 1,
            cap_id: [0xcd; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: subject.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: Scope {
                workspace_id: None,
                project_id: None,
                task_id: None,
                tool_class_mask: ToolClass::all_phase1_mask(),
                providers: None,
            },
            caveats: vec![Caveat::MaxTotalSpend(FRAC)],
            budget_account: 0,
            not_before: 0,
            not_after: 2_000_000,
            nonce: 1,
        };
        let token = CapabilityToken::sign(body, &issuer).unwrap();
        let token_hex = format!("0x{}", hex::encode(borsh::to_vec(&token).unwrap()));

        let v = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "submit-mint-cap".into(),
            "--token-hex".into(),
            token_hex,
            "--apply-local".into(),
        ])
        .unwrap();

        assert_eq!(v.get("mode").and_then(Value::as_str), Some("apply-local"));
        assert_eq!(v.get("registered").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn submit_revoke_cap_apply_local_after_mint_cap() {
        use ed25519_dalek::SigningKey;
        use fractal_core::WalletRevokeCapabilitySignBody;
        use fractal_wallet::{
            capability::{CapabilitySignBody, CapabilityToken},
            caveat::Caveat,
            policy::builtins::FRAC,
            types::{Scope, ToolClass},
        };

        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let subject = SigningKey::generate(&mut rng);
        let cap_id = [0xde; 32];
        let body = CapabilitySignBody {
            version: 1,
            cap_id,
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: subject.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: Scope {
                workspace_id: None,
                project_id: None,
                task_id: None,
                tool_class_mask: ToolClass::all_phase1_mask(),
                providers: None,
            },
            caveats: vec![Caveat::MaxTotalSpend(FRAC)],
            budget_account: 0,
            not_before: 0,
            not_after: 2_000_000,
            nonce: 1,
        };
        let token = CapabilityToken::sign(body, &issuer).unwrap();
        let token_hex = format!("0x{}", hex::encode(borsh::to_vec(&token).unwrap()));
        let issuer_secret = format!("0x{}", hex::encode(issuer.to_bytes()));

        crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "submit-mint-cap".into(),
            "--token-hex".into(),
            token_hex,
            "--apply-local".into(),
        ])
        .unwrap();

        let revoke = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "submit-revoke-cap".into(),
            "--cap-id".into(),
            format!("0x{}", hex::encode(cap_id)),
            "--issuer-secret".into(),
            issuer_secret,
            "--apply-local".into(),
            "--nonce".into(),
            "0".into(),
        ])
        .unwrap_err();
        assert!(
            revoke.contains("WalletCapabilityNotFound"),
            "each --apply-local uses an isolated state: {revoke}"
        );

        let signer = HARDHAT_DEFAULT_SIGNER_0;
        let mut state = funded_local_state(signer, u128::MAX / 4, 1_000_000);
        let proof = fractal_core::mint_revocation_proof_bytes(&state, &token).unwrap();
        apply_block(
            &mut state,
            &[Transaction {
                signer,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletMintCapabilityV1 {
                    parent_cap_id: None,
                    child_token_borsh: borsh::to_vec(&token).unwrap(),
                    budget_seed: None,
                    revocation_proof_borsh: proof,
                }),
            }],
        )
        .unwrap();
        let sign_body = WalletRevokeCapabilitySignBody {
            cap_id,
            reason_code: 1,
            cascade: true,
            chain_id: 41,
        };
        let sig = issuer.sign(&borsh::to_vec(&sign_body).unwrap()).to_bytes();
        apply_block(
            &mut state,
            &[Transaction {
                signer,
                nonce: 1,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletRevokeCapabilityV1 {
                    cap_id,
                    reason_code: 1,
                    cascade: true,
                    issuer_sig: sig,
                }),
            }],
        )
        .unwrap();
        assert!(state.wallet_revocation_entries.get(&cap_id).unwrap().cascade);
    }

    #[test]
    fn submit_create_and_fund_budget_apply_local() {
        let signer = HARDHAT_DEFAULT_SIGNER_0;
        let mut state = funded_local_state(signer, u128::MAX / 4, 1_000_000);
        let txs = [
            Transaction {
                signer,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletCreateBudgetAccountV1 {
                    parent: None,
                    initial_deposit: 5 * WEI_PER_FRAC,
                }),
            },
            Transaction {
                signer,
                nonce: 1,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletFundBudgetAccountV1 {
                    budget: 1,
                    amount: WEI_PER_FRAC,
                    source_budget: None,
                }),
            },
        ];
        apply_block(&mut state, &txs).unwrap();
        assert_eq!(
            state.wallet_budgets.get(&1).unwrap().total_deposited,
            6 * WEI_PER_FRAC
        );

        let create_cli = crate::run_argv_value(&[
            "fractal-wallet-cli".into(),
            "chain".into(),
            "submit-create-budget".into(),
            "--initial-deposit".into(),
            (5 * WEI_PER_FRAC).to_string(),
            "--apply-local".into(),
        ])
        .unwrap();
        assert_eq!(
            create_cli.get("budgetId").and_then(Value::as_u64),
            Some(1)
        );
    }
}

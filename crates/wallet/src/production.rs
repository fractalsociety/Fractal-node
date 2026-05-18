//! Production tool-class paths (`docs/wallet.md` §8–§9, §13.3): TEE attestation,
//! class-specific receipt checks, and verifier sampling.

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::caveat::Caveat;
use crate::challenge::ChallengeKind;
use crate::market::ToolIntent;
use crate::task_receipt::{TeeAttestation, ToolReceipt, ToolReceiptVerifyError};
use crate::types::{
    Amount, IntentId, PublicKey, TaskId, TeeType, TimestampMs, ToolClass, VerificationTier,
};

/// Wire format for production TEE quotes (`docs/wallet.md` §9.1, §25.2).
///
/// Real Intel TDX / AMD SNP / Nitro quotes are opaque; this structured envelope is what
/// `verify_tee_attestation` checks today. Operators bind `report_data` to
/// `payload_commitment` (or provider pubkey) before posting receipts.
pub const TEE_QUOTE_MAGIC: &[u8; 12] = b"FRAC_TEE_V1\0";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TeeQuoteV1 {
    pub tee_type: TeeType,
    pub measurement: [u8; 32],
    pub report_data: [u8; 32],
    pub enclave_pubkey: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassVerificationMethod {
    TrustedSignature,
    OptimisticChallenge,
    TeeAttestation,
    ReplicatedQuorum,
    ZkProof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeteringRequirements {
    pub require_input_tokens: bool,
    pub require_output_tokens: bool,
    pub require_wall_duration: bool,
    pub require_bytes_metered: bool,
}

impl ToolClass {
    /// Class-specific metering expectations (`docs/wallet.md` §8.1, §9.1).
    #[must_use]
    pub fn metering_requirements(self) -> MeteringRequirements {
        match self {
            Self::LlmInference | Self::Embedding => MeteringRequirements {
                require_input_tokens: true,
                require_output_tokens: true,
                require_wall_duration: false,
                require_bytes_metered: false,
            },
            Self::Browser | Self::WebScrape | Self::VectorSearch | Self::Ocr => {
                MeteringRequirements {
                    require_input_tokens: false,
                    require_output_tokens: false,
                    require_wall_duration: true,
                    require_bytes_metered: true,
                }
            }
            Self::GpuJob | Self::CodeExecution | Self::TestRunner => MeteringRequirements {
                require_input_tokens: false,
                require_output_tokens: false,
                require_wall_duration: true,
                require_bytes_metered: false,
            },
            Self::FileStorage => MeteringRequirements {
                require_input_tokens: false,
                require_output_tokens: false,
                require_wall_duration: false,
                require_bytes_metered: true,
            },
            Self::GithubRead | Self::GithubWrite | Self::DatabaseQuery | Self::EmailSend => {
                MeteringRequirements {
                    require_input_tokens: false,
                    require_output_tokens: false,
                    require_wall_duration: true,
                    require_bytes_metered: false,
                }
            }
        }
    }

    #[must_use]
    pub fn production_verification_method(self, tier: VerificationTier) -> ClassVerificationMethod {
        match tier {
            VerificationTier::Trusted => ClassVerificationMethod::TrustedSignature,
            VerificationTier::Optimistic => ClassVerificationMethod::OptimisticChallenge,
            VerificationTier::Attested => ClassVerificationMethod::TeeAttestation,
            VerificationTier::Replicated => ClassVerificationMethod::ReplicatedQuorum,
            VerificationTier::Proven => ClassVerificationMethod::ZkProof,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TeeAttestationError {
    #[error("attestation missing")]
    Missing,
    #[error("quote too short")]
    QuoteTooShort,
    #[error("invalid TEE quote magic")]
    BadMagic,
    #[error("quote borsh decode failed")]
    BadEncoding,
    #[error("tee_type mismatch: expected {expected:?}, got {got:?}")]
    TeeTypeMismatch { expected: TeeType, got: TeeType },
    #[error("measurement mismatch")]
    MeasurementMismatch,
    #[error("report_data does not bind payload commitment")]
    ReportDataMismatch,
}

/// Encode a production TEE quote for [`TeeAttestation::quote`].
pub fn encode_tee_quote_v1(q: &TeeQuoteV1) -> Result<Vec<u8>, std::io::Error> {
    let mut out = Vec::with_capacity(TEE_QUOTE_MAGIC.len() + 128);
    out.extend_from_slice(TEE_QUOTE_MAGIC);
    out.extend_from_slice(&borsh::to_vec(q)?);
    Ok(out)
}

/// Verify structured TEE quote bytes and optional binding to `payload_commitment`.
pub fn verify_tee_attestation(
    att: &TeeAttestation,
    expected_tee: TeeType,
    expected_measurement: Option<&[u8; 32]>,
    payload_commitment: &[u8; 32],
) -> Result<TeeQuoteV1, TeeAttestationError> {
    if att.tee_type != expected_tee {
        return Err(TeeAttestationError::TeeTypeMismatch {
            expected: expected_tee,
            got: att.tee_type,
        });
    }
    let q = decode_tee_quote_v1(&att.quote)?;
    if let Some(m) = expected_measurement {
        if &q.measurement != m {
            return Err(TeeAttestationError::MeasurementMismatch);
        }
    }
    if q.report_data != *payload_commitment {
        return Err(TeeAttestationError::ReportDataMismatch);
    }
    Ok(q)
}

pub fn decode_tee_quote_v1(quote: &[u8]) -> Result<TeeQuoteV1, TeeAttestationError> {
    if quote.len() < TEE_QUOTE_MAGIC.len() {
        return Err(TeeAttestationError::QuoteTooShort);
    }
    if &quote[..TEE_QUOTE_MAGIC.len()] != TEE_QUOTE_MAGIC {
        return Err(TeeAttestationError::BadMagic);
    }
    borsh::from_slice(&quote[TEE_QUOTE_MAGIC.len()..]).map_err(|_| TeeAttestationError::BadEncoding)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionVerifyContext<'a> {
    pub intent: &'a ToolIntent,
    pub receipt: &'a ToolReceipt,
    pub provider_pk: &'a PublicKey,
    pub caveats: &'a [Caveat],
    pub required_attestations: &'a BTreeSet<(ToolClass, TeeType)>,
    pub expected_measurement: Option<[u8; 32]>,
    pub now_ms: TimestampMs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProductionVerifyReport {
    pub provider_signature_ok: bool,
    pub intent_binding_ok: bool,
    pub tier_ok: bool,
    pub metering_ok: bool,
    pub attestation_ok: bool,
    pub caveats_ok: bool,
    pub method: Option<ClassVerificationMethod>,
}

impl ProductionVerifyReport {
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.provider_signature_ok
            && self.intent_binding_ok
            && self.tier_ok
            && self.metering_ok
            && self.attestation_ok
            && self.caveats_ok
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductionVerifyError {
    #[error("tool receipt: {0}")]
    Receipt(#[from] ToolReceiptVerifyError),
    #[error("tee attestation: {0}")]
    Tee(#[from] TeeAttestationError),
    #[error("production checks failed")]
    ChecksFailed,
}

/// Full production-path verification for a matched intent + receipt.
pub fn verify_production_tool_receipt(
    ctx: &ProductionVerifyContext<'_>,
) -> Result<ProductionVerifyReport, ProductionVerifyError> {
    let class = ctx.intent.body.tool_class;
    let tier = ctx.intent.body.verification_tier;
    let mut report = ProductionVerifyReport {
        method: Some(class.production_verification_method(tier)),
        ..Default::default()
    };

    report.provider_signature_ok = ctx.receipt.verify_provider(ctx.provider_pk).is_ok();
    report.intent_binding_ok = ctx.receipt.body.intent_id == ctx.intent.body.intent_id
        && ctx.receipt.body.tool_class == class
        && ctx.receipt.body.task_id == ctx.intent.body.task_id
        && ctx.receipt.body.agent_session == ctx.intent.body.agent_session;

    report.tier_ok = tier_rank(tier) >= tier_rank(class.default_verification_tier());

    report.metering_ok = check_metering(class, &ctx.receipt);

    report.attestation_ok = check_attestation(
        class,
        tier,
        ctx.receipt,
        ctx.required_attestations,
        ctx.expected_measurement.as_ref(),
        &ctx.intent.body.payload_commitment,
    );

    report.caveats_ok = check_caveats(ctx.caveats, class, ctx.receipt);

    if !report.all_ok() {
        return Err(ProductionVerifyError::ChecksFailed);
    }
    Ok(report)
}

fn tier_rank(t: VerificationTier) -> u8 {
    t as u8
}

fn check_metering(class: ToolClass, receipt: &ToolReceipt) -> bool {
    let req = class.metering_requirements();
    let m = &receipt.body.metering;
    if req.require_input_tokens && m.input_tokens == 0 {
        return false;
    }
    if req.require_output_tokens && m.output_tokens == 0 {
        return false;
    }
    if req.require_wall_duration && m.wall_duration_ms == 0 {
        return false;
    }
    if req.require_bytes_metered && m.bytes_metered == 0 {
        return false;
    }
    true
}

fn check_attestation(
    class: ToolClass,
    tier: VerificationTier,
    receipt: &ToolReceipt,
    required: &BTreeSet<(ToolClass, TeeType)>,
    expected_measurement: Option<&[u8; 32]>,
    payload_commitment: &[u8; 32],
) -> bool {
    let needs_tee = tier == VerificationTier::Attested || required.iter().any(|(c, _)| *c == class);
    if !needs_tee {
        return true;
    }
    let Some(att) = &receipt.body.attestation else {
        return false;
    };
    let expected_tee = required
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, t)| *t)
        .unwrap_or(att.tee_type);
    verify_tee_attestation(att, expected_tee, expected_measurement, payload_commitment).is_ok()
}

fn check_caveats(caveats: &[Caveat], class: ToolClass, receipt: &ToolReceipt) -> bool {
    for c in caveats {
        match c {
            Caveat::TeeAttestationRequired { class: c, tee } if *c == class => {
                let Some(att) = &receipt.body.attestation else {
                    return false;
                };
                if att.tee_type != *tee {
                    return false;
                }
                if verify_tee_attestation(att, *tee, None, &receipt.body.payload_commitment)
                    .is_err()
                {
                    return false;
                }
            }
            Caveat::OutputCommitmentRequired(c) if *c == class => {
                if receipt.body.output_commitment == [0u8; 32] {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Map a failed production check to an on-chain challenge kind (`docs/wallet.md` §9.3).
#[must_use]
pub fn challenge_kind_for_production_failure(
    report: &ProductionVerifyReport,
) -> Option<ChallengeKind> {
    if !report.attestation_ok {
        return Some(ChallengeKind::Unattested);
    }
    if !report.metering_ok {
        return Some(ChallengeKind::Overcharged);
    }
    if !report.intent_binding_ok {
        return Some(ChallengeKind::WrongOutput);
    }
    if !report.provider_signature_ok {
        return Some(ChallengeKind::NotExecuted);
    }
    None
}

/// Verifier pool entry for §13.3 sampling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierCandidate {
    pub pubkey: PublicKey,
    pub stake_weight: u128,
    pub reputation_milli: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifierSamplingConfig {
    /// Minimum task / intent `max_price` before sampling applies.
    pub min_task_value: Amount,
    /// Basis points (10_000 = always sample when above threshold).
    pub sample_rate_bps: u16,
}

impl Default for VerifierSamplingConfig {
    fn default() -> Self {
        Self {
            min_task_value: 10 * crate::policy::builtins::FRAC,
            sample_rate_bps: 10_000,
        }
    }
}

/// Pseudo-random sampling gate using a VRF-beacon stand-in (`docs/wallet.md` §13.3).
#[must_use]
pub fn should_sample_verifier(
    vrf_beacon: &[u8; 32],
    task_id: TaskId,
    max_price: Amount,
    cfg: &VerifierSamplingConfig,
) -> bool {
    if max_price < cfg.min_task_value {
        return false;
    }
    if cfg.sample_rate_bps >= 10_000 {
        return true;
    }
    let roll = sample_word(vrf_beacon, task_id, [0u8; 32]) % 10_000;
    roll < u128::from(cfg.sample_rate_bps)
}

/// Weighted verifier selection: `weight = stake_weight * max(reputation_milli, 1)`.
#[must_use]
pub fn select_verifier_weighted(
    vrf_beacon: &[u8; 32],
    task_id: TaskId,
    intent_id: IntentId,
    pool: &[VerifierCandidate],
) -> Option<PublicKey> {
    if pool.is_empty() {
        return None;
    }
    let mut total = 0u128;
    let mut weights = Vec::with_capacity(pool.len());
    for v in pool {
        let w = v
            .stake_weight
            .saturating_mul(u128::from(v.reputation_milli.max(1)));
        weights.push(w);
        total = total.saturating_add(w);
    }
    if total == 0 {
        let idx = (sample_word(vrf_beacon, task_id, intent_id) as usize) % pool.len();
        return Some(pool[idx].pubkey);
    }
    let pick = sample_word(vrf_beacon, task_id, intent_id) % total;
    let mut acc = 0u128;
    for (v, w) in pool.iter().zip(weights.iter()) {
        acc = acc.saturating_add(*w);
        if pick < acc {
            return Some(v.pubkey);
        }
    }
    pool.last().map(|v| v.pubkey)
}

fn sample_word(beacon: &[u8; 32], task_id: TaskId, intent_id: IntentId) -> u128 {
    let mut buf = [0u8; 96];
    buf[..32].copy_from_slice(beacon);
    buf[32..40].copy_from_slice(&task_id.to_le_bytes());
    buf[64..].copy_from_slice(&intent_id);
    let h = blake3::hash(&buf);
    u128::from_le_bytes(h.as_bytes()[..16].try_into().expect("16 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use crate::market::ToolIntentBody;
    use crate::task_receipt::{MeteringRecord, ToolReceiptBody};
    fn sample_receipt(
        intent: &ToolIntent,
        provider: &SigningKey,
        att: Option<TeeAttestation>,
    ) -> ToolReceipt {
        let provider_pk = provider.verifying_key().to_bytes();
        let body = ToolReceiptBody {
            intent_id: intent.body.intent_id,
            task_id: intent.body.task_id,
            agent_session: intent.body.agent_session,
            provider_id: crate::market::provider_id_from_public_key(&provider_pk),
            tool_class: intent.body.tool_class,
            payload_commitment: intent.body.payload_commitment,
            output_commitment: [0xee; 32],
            output_pointer: "da://out/1".into(),
            metering: MeteringRecord {
                input_tokens: 10,
                output_tokens: 20,
                wall_duration_ms: 50,
                bytes_metered: 100,
            },
            cost: 5,
            started_at: 1,
            completed_at: 2,
            attestation: att,
        };
        ToolReceipt::sign_new(body, provider).unwrap()
    }

    #[test]
    fn tee_quote_round_trip_and_bind() {
        let q = TeeQuoteV1 {
            tee_type: TeeType::IntelTdx,
            measurement: [0x11; 32],
            report_data: [0x22; 32],
            enclave_pubkey: [0x33; 32],
        };
        let bytes = encode_tee_quote_v1(&q).unwrap();
        let att = TeeAttestation {
            tee_type: TeeType::IntelTdx,
            quote: bytes,
        };
        verify_tee_attestation(&att, TeeType::IntelTdx, Some(&[0x11; 32]), &[0x22; 32]).unwrap();
    }

    #[test]
    fn production_verify_attested_github_write() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let provider = SigningKey::generate(&mut rng);
        let intent_id = [0xabu8; 32];
        let payload = [0xcd; 32];
        let intent = ToolIntent::sign(
            ToolIntentBody {
                intent_id,
                agent_session: agent.verifying_key().to_bytes(),
                task_id: 9,
                tool_class: ToolClass::GithubWrite,
                payload_commitment: payload,
                max_price: 100,
                verification_tier: VerificationTier::Attested,
                deadline_ms: 9_999,
                nonce: 1,
            },
            &agent,
        )
        .unwrap();
        let q = TeeQuoteV1 {
            tee_type: TeeType::IntelTdx,
            measurement: [0x01; 32],
            report_data: payload,
            enclave_pubkey: [0u8; 32],
        };
        let att = TeeAttestation {
            tee_type: TeeType::IntelTdx,
            quote: encode_tee_quote_v1(&q).unwrap(),
        };
        let receipt = sample_receipt(&intent, &provider, Some(att));
        let provider_pk = provider.verifying_key().to_bytes();
        let required = BTreeSet::from([(ToolClass::GithubWrite, TeeType::IntelTdx)]);
        let ctx = ProductionVerifyContext {
            intent: &intent,
            receipt: &receipt,
            provider_pk: &provider_pk,
            caveats: &[],
            required_attestations: &required,
            expected_measurement: Some([0x01; 32]),
            now_ms: 100,
        };
        let report = verify_production_tool_receipt(&ctx).unwrap();
        assert!(report.all_ok());
        assert_eq!(report.method, Some(ClassVerificationMethod::TeeAttestation));
    }

    #[test]
    fn verifier_sampling_is_deterministic_and_weighted() {
        let beacon = [0x42u8; 32];
        let task = 7u64;
        let intent = [0x11u8; 32];
        let pool = vec![
            VerifierCandidate {
                pubkey: [1u8; 32],
                stake_weight: 1,
                reputation_milli: 1,
            },
            VerifierCandidate {
                pubkey: [2u8; 32],
                stake_weight: 100,
                reputation_milli: 500,
            },
        ];
        let a = select_verifier_weighted(&beacon, task, intent, &pool).unwrap();
        let b = select_verifier_weighted(&beacon, task, intent, &pool).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, [2u8; 32]);
        let cfg = VerifierSamplingConfig {
            min_task_value: 1,
            sample_rate_bps: 10_000,
        };
        assert!(should_sample_verifier(&beacon, task, 10, &cfg));
    }
}

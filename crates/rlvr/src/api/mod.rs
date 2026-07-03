//! Local API surface for the RLVR UI flow.

pub mod training_report;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    chain::committed::{RlvrCommittedProofIndex, RlvrProofBlockReference, RlvrProofStatus},
    default_target_modules, export_adapter_bundle, generate_route_correctness_rubric,
    generate_tool_use_rubric, hash_bytes, load_adapter_bundle, read_eval_traces, run_rollout_batch,
    synthesize_weights, train_grpo_adapter, write_eval_report, write_rollout_traces, AdapterConfig,
    AdapterExportInput, DeterministicLocalActorRuntime, DialogueTrace, GrpoTrainerInput,
    ModelInventoryItem, NodeSigningKey, RlvrError, RlvrProofObject, RlvrProofPool, RlvrProofType,
    RolloutRunnerInput, RoutePolicy, RouteTraceRow, SimulatorMode, ToolInventoryItem,
    TraceHashCommitment, TrainingItem, DEFAULT_ADAPTER_RANK, DEFAULT_MODEL_DIM,
    MVP_REWARD_POLICY_V01_ID,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitRlvrProofResponse {
    pub method: String,
    pub proof_hash: String,
    pub proof_type: RlvrProofType,
    pub node_id: String,
    pub pending_proofs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTraceSummary {
    pub trace_id: String,
    pub task_id: String,
    pub trace_hash: String,
    pub redacted_trace_hash: String,
    pub verifier_outputs_hash: String,
    pub reward_vector_hash: String,
    pub privacy_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLocalTracesResponse {
    pub method: String,
    pub trace_dir: String,
    pub traces: Vec<LocalTraceSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MakeRubricsRequest {
    pub trace: RouteTraceRow,
    pub visible_prompt: Option<String>,
    pub route_policy: RoutePolicy,
    pub models: Vec<ModelInventoryItem>,
    pub tools: Vec<ToolInventoryItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MakeRubricsResponse {
    pub method: String,
    pub rubrics: Vec<TrainingItem>,
    pub rubric_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRolloutRequest {
    pub task_count: usize,
    pub out_dir: PathBuf,
    pub actor_id: String,
    pub trace_id_prefix: String,
    pub max_turns: u32,
    pub simulator_mode: SimulatorMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRolloutResponse {
    pub method: String,
    pub out_dir: String,
    pub trace_count: usize,
    pub trace_paths: Vec<String>,
    pub traces: Vec<LocalTraceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvalRequest {
    pub input: PathBuf,
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvalResponse {
    pub method: String,
    pub json_path: String,
    pub html_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAdapterRequest {
    pub adapter_id: String,
    pub base_model_id: String,
    pub out_dir: PathBuf,
    pub rank: Option<u32>,
    pub registry_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAdapterResponse {
    pub method: String,
    pub adapter_id: String,
    pub adapter_hash: String,
    pub reward_policy_hash: String,
    pub out_dir: String,
    pub manifest_path: String,
    pub file_count: usize,
    pub registered: bool,
    pub loadable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateProofObjectRequest {
    pub trace: DialogueTrace,
    pub proof_type: RlvrProofType,
    pub reward_policy_hash: String,
    pub route_policy_hash: String,
    pub model_id_hash: String,
    pub adapter_hash: Option<String>,
    pub eval_result_hash: Option<String>,
    pub timestamp_ms: u64,
    pub node_id: String,
    pub node_seed: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProofObjectResponse {
    pub method: String,
    pub proof_hash: String,
    pub proof: RlvrProofObject,
    pub proof_json: Vec<u8>,
}

pub fn fractal_list_local_traces(
    trace_dir: impl AsRef<Path>,
) -> Result<ListLocalTracesResponse, RlvrError> {
    let trace_dir = trace_dir.as_ref();
    let traces = read_eval_traces(trace_dir)?
        .iter()
        .map(local_trace_summary)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ListLocalTracesResponse {
        method: "fractal_listLocalRlvrTraces".into(),
        trace_dir: trace_dir.to_string_lossy().into_owned(),
        traces,
    })
}

pub fn fractal_make_rlvr_rubrics(
    request: MakeRubricsRequest,
) -> Result<MakeRubricsResponse, RlvrError> {
    let route = generate_route_correctness_rubric(crate::RouteCorrectnessRubricInput {
        trace: request.trace.clone(),
        visible_prompt: request.visible_prompt.clone(),
        models: request.models.clone(),
        tools: request.tools.clone(),
        route_policy: request.route_policy.clone(),
    })?;
    let tool = generate_tool_use_rubric(crate::ToolUseRubricInput {
        trace: request.trace,
        visible_prompt: request.visible_prompt,
        tools: request.tools,
        route_policy: request.route_policy,
    })?;
    let rubrics = vec![route, tool];
    let rubric_hashes = rubrics
        .iter()
        .map(|rubric| rubric.stable_hash())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MakeRubricsResponse {
        method: "fractal_makeRlvrRubrics".into(),
        rubrics,
        rubric_hashes,
    })
}

pub fn fractal_run_rlvr_rollout(
    request: RunRolloutRequest,
) -> Result<RunRolloutResponse, RlvrError> {
    if request.task_count == 0 {
        return Err(RlvrError::Config(
            "rollout API task_count must be greater than zero".into(),
        ));
    }
    let runtime = DeterministicLocalActorRuntime::new(request.actor_id.clone());
    let report = run_rollout_batch(
        &runtime,
        RolloutRunnerInput {
            tasks: crate::demo_rollout_tasks(request.task_count),
            actor_id: request.actor_id,
            trace_id_prefix: request.trace_id_prefix,
            max_turns: request.max_turns,
            simulator_mode: request.simulator_mode,
        },
    )?;
    let trace_paths = write_rollout_traces(&report, &request.out_dir)?
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let traces = report
        .traces
        .iter()
        .map(local_trace_summary)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RunRolloutResponse {
        method: "fractal_runRlvrRollout".into(),
        out_dir: request.out_dir.to_string_lossy().into_owned(),
        trace_count: trace_paths.len(),
        trace_paths,
        traces,
    })
}

pub fn fractal_run_rlvr_eval(request: RunEvalRequest) -> Result<RunEvalResponse, RlvrError> {
    let files = write_eval_report(&request.input, &request.out_dir)?;
    Ok(RunEvalResponse {
        method: "fractal_runRlvrEval".into(),
        json_path: files.json_path,
        html_path: files.html_path,
    })
}

pub fn fractal_export_rlvr_adapter(
    request: ExportAdapterRequest,
) -> Result<ExportAdapterResponse, RlvrError> {
    require_non_empty("export_adapter.adapter_id", &request.adapter_id)?;
    require_non_empty("export_adapter.base_model_id", &request.base_model_id)?;
    let rank = request.rank.unwrap_or(DEFAULT_ADAPTER_RANK);
    if rank == 0 {
        return Err(RlvrError::Config(
            "export_adapter.rank must be greater than zero".into(),
        ));
    }

    let runtime = DeterministicLocalActorRuntime::new(&request.base_model_id);
    let rollouts = run_rollout_batch(
        &runtime,
        RolloutRunnerInput {
            tasks: crate::demo_rollout_tasks(2),
            actor_id: request.base_model_id.clone(),
            trace_id_prefix: "api-export-rollout".into(),
            max_turns: 3,
            simulator_mode: SimulatorMode::Clean,
        },
    )?;
    let mut traces = rollouts.traces;
    traces.extend(traces.clone());
    let report = train_grpo_adapter(GrpoTrainerInput {
        base_model_id: request.base_model_id.clone(),
        adapter_id: request.adapter_id.clone(),
        rollouts: traces,
        output_dir: std::env::temp_dir(),
        learning_rate: 0.05,
        epochs: 2,
    })?;
    let weights = synthesize_weights(&report, rank, default_target_modules(), DEFAULT_MODEL_DIM)?;
    let config = AdapterConfig {
        adapter_id: weights.adapter_id.clone(),
        base_model_id: weights.base_model_id.clone(),
        training_mode: weights.training_mode,
        rank: weights.rank,
        target_modules: weights.target_modules.clone(),
        max_turns: 3,
        data_local_only: true,
        base_model_hash: hash_bytes(weights.base_model_id.as_bytes()),
        created_from_checkpoint: Some(report.checkpoint_path.clone()),
    };
    let export = export_adapter_bundle(
        AdapterExportInput {
            weights,
            config,
            reward_version: MVP_REWARD_POLICY_V01_ID.into(),
            timestamp_ms: now_ms(),
            registry_path: request.registry_path,
        },
        &report,
        &request.out_dir,
    )?;
    let loadable = load_adapter_bundle(&export.out_dir).is_ok();
    Ok(ExportAdapterResponse {
        method: "fractal_exportRlvrAdapter".into(),
        adapter_id: export.adapter_id,
        adapter_hash: export.adapter_hash,
        reward_policy_hash: export.reward_policy_hash,
        out_dir: export.out_dir.to_string_lossy().into_owned(),
        manifest_path: export.manifest_path.to_string_lossy().into_owned(),
        file_count: export.files.len(),
        registered: export.registered,
        loadable,
    })
}

pub fn fractal_create_rlvr_proof_object(
    request: CreateProofObjectRequest,
) -> Result<CreateProofObjectResponse, RlvrError> {
    require_non_empty("create_proof.node_id", &request.node_id)?;
    if request.timestamp_ms == 0 {
        return Err(RlvrError::Config(
            "create_proof.timestamp_ms must be greater than zero".into(),
        ));
    }
    let commitment: TraceHashCommitment = request.trace.trace_hash_commitment()?;
    let key = NodeSigningKey::from_seed(request.node_id, &request.node_seed)?;
    let mut proof = RlvrProofObject::from_trace_commitment(
        request.proof_type,
        &commitment,
        request.reward_policy_hash,
        request.route_policy_hash,
        request.model_id_hash,
        request.timestamp_ms,
        "unsigned-placeholder",
    );
    if let Some(adapter_hash) = request.adapter_hash {
        proof = proof.with_adapter_hash(adapter_hash);
    }
    if let Some(eval_result_hash) = request.eval_result_hash {
        proof = proof.with_eval_result_hash(eval_result_hash);
    }
    proof = proof.sign_with_node_key(&key)?;
    let proof_hash = proof.proof_hash()?;
    let proof_json = serde_json::to_vec(&proof)?;
    Ok(CreateProofObjectResponse {
        method: "fractal_createRlvrProofObject".into(),
        proof_hash,
        proof,
        proof_json,
    })
}

fn local_trace_summary(trace: &DialogueTrace) -> Result<LocalTraceSummary, RlvrError> {
    let commitment = trace.trace_hash_commitment()?;
    Ok(LocalTraceSummary {
        trace_id: commitment.trace_id,
        task_id: commitment.task_id,
        trace_hash: commitment.trace_hash,
        redacted_trace_hash: commitment.redacted_trace_hash,
        verifier_outputs_hash: commitment.verifier_outputs_hash,
        reward_vector_hash: commitment.reward_vector_hash,
        privacy_tags: commitment.privacy_tags,
    })
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(1)
        .max(1)
}

/// Local implementation of the `fractal_submitRlvrProof` RPC contract.
///
/// `proof_json_or_canonical_bytes` is JSON for [`RlvrProofObject`]. The same
/// bytes are accepted whether they came from normal proof JSON serialization or
/// [`RlvrProofObject::canonical_bytes`]. Raw user fields are rejected by the
/// proof schema (`deny_unknown_fields`), hash-only validation, and local
/// signature verification before insertion.
pub fn fractal_submit_rlvr_proof(
    pool: &mut RlvrProofPool,
    proof_json_or_canonical_bytes: impl AsRef<[u8]>,
) -> Result<SubmitRlvrProofResponse, RlvrError> {
    let proof: RlvrProofObject = serde_json::from_slice(proof_json_or_canonical_bytes.as_ref())?;
    proof.verify_node_signature()?;
    let proof_hash = pool.insert(proof.clone())?;
    Ok(SubmitRlvrProofResponse {
        method: "fractal_submitRlvrProof".into(),
        proof_hash,
        proof_type: proof.proof_type,
        node_id: proof
            .node_id
            .clone()
            .ok_or_else(|| RlvrError::Config("verified proof is missing node_id".into()))?,
        pending_proofs: pool.len(),
    })
}

/// Response for [`fractal_get_rlvr_proof`]. Carries proof type, status,
/// timestamp, node id, and (when committed) a block reference — but never raw
/// trace data. `found` is `false` when the hash is neither pending nor committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRlvrProofResponse {
    pub method: String,
    pub proof_hash: String,
    pub found: bool,
    pub status: RlvrProofStatus,
    pub proof_type: Option<RlvrProofType>,
    pub timestamp_ms: Option<u64>,
    pub node_id: Option<String>,
    pub block_reference: Option<RlvrProofBlockReference>,
}

/// Local implementation of the `fractal_getRlvrProof` RPC contract (RLVR-046).
///
/// Looks up `proof_hash` in `pending` first by the committed index (a committed
/// proof wins over a still-pending copy), then the pending pool. Returns a
/// status summary: `Committed` carries the block reference, `Pending` does not,
/// and `NotFound` has `found == false`. No raw trace data is ever returned —
/// only hashes and metadata derived from the hash-only [`RlvrProofObject`].
pub fn fractal_get_rlvr_proof(
    pending: &RlvrProofPool,
    committed: &RlvrCommittedProofIndex,
    proof_hash: &str,
) -> Result<GetRlvrProofResponse, RlvrError> {
    validate_hex_hash("proof_hash", proof_hash)?;
    if let Some(entry) = committed.get(proof_hash) {
        let proof = &entry.proof;
        return Ok(GetRlvrProofResponse {
            method: "fractal_getRlvrProof".into(),
            proof_hash: proof_hash.into(),
            found: true,
            status: RlvrProofStatus::Committed,
            proof_type: Some(proof.proof_type),
            timestamp_ms: Some(proof.timestamp_ms),
            node_id: proof.node_id.clone(),
            block_reference: Some(entry.block.clone()),
        });
    }
    if let Some(proof) = pending.get(proof_hash) {
        return Ok(GetRlvrProofResponse {
            method: "fractal_getRlvrProof".into(),
            proof_hash: proof_hash.into(),
            found: true,
            status: RlvrProofStatus::Pending,
            proof_type: Some(proof.proof_type),
            timestamp_ms: Some(proof.timestamp_ms),
            node_id: proof.node_id.clone(),
            block_reference: None,
        });
    }
    Ok(GetRlvrProofResponse {
        method: "fractal_getRlvrProof".into(),
        proof_hash: proof_hash.into(),
        found: false,
        status: RlvrProofStatus::NotFound,
        proof_type: None,
        timestamp_ms: None,
        node_id: None,
        block_reference: None,
    })
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hash_bytes, route_policy_hash, NodeSigningKey, RlvrProofPool, RouteTraceInput,
        TraceHashCommitment,
    };

    #[test]
    fn local_api_runs_rollout_lists_traces_eval_creates_and_submits_proof() {
        let dir = temp_dir("local-api-flow");
        let rollout_dir = dir.join("rollouts");
        let eval_dir = dir.join("eval");
        let _ = std::fs::remove_dir_all(&dir);

        let rollout = fractal_run_rlvr_rollout(RunRolloutRequest {
            task_count: 2,
            out_dir: rollout_dir.clone(),
            actor_id: "local-api-model".into(),
            trace_id_prefix: "api-rollout".into(),
            max_turns: 3,
            simulator_mode: SimulatorMode::Clean,
        })
        .unwrap();
        assert_eq!(rollout.method, "fractal_runRlvrRollout");
        assert_eq!(rollout.trace_count, 2);
        assert_eq!(rollout.trace_paths.len(), 2);
        assert!(rollout
            .traces
            .iter()
            .all(|trace| !trace.trace_hash.is_empty()));

        let listed = fractal_list_local_traces(&rollout_dir).unwrap();
        assert_eq!(listed.method, "fractal_listLocalRlvrTraces");
        assert_eq!(listed.traces.len(), 2);
        assert_eq!(listed.traces[0].trace_hash, rollout.traces[0].trace_hash);

        let eval = fractal_run_rlvr_eval(RunEvalRequest {
            input: rollout_dir.clone(),
            out_dir: eval_dir.clone(),
        })
        .unwrap();
        assert_eq!(eval.method, "fractal_runRlvrEval");
        assert!(std::path::Path::new(&eval.json_path).exists());
        assert!(std::path::Path::new(&eval.html_path).exists());

        let traces = read_eval_traces(&rollout_dir).unwrap();
        let proof = fractal_create_rlvr_proof_object(CreateProofObjectRequest {
            trace: traces[0].clone(),
            proof_type: RlvrProofType::ProofOfRoute,
            reward_policy_hash: hash_bytes(b"reward-policy"),
            route_policy_hash: route_policy_hash(&RoutePolicy::default()).unwrap(),
            model_id_hash: hash_bytes(b"local-api-model"),
            adapter_hash: None,
            eval_result_hash: None,
            timestamp_ms: 1_700_000_000_000,
            node_id: "api-node".into(),
            node_seed: b"api-node-secret".to_vec(),
        })
        .unwrap();
        assert_eq!(proof.method, "fractal_createRlvrProofObject");
        assert_eq!(proof.proof_hash, proof.proof.proof_hash().unwrap());
        assert!(serde_json::from_slice::<RlvrProofObject>(&proof.proof_json).is_ok());

        let mut pool = RlvrProofPool::new();
        let submitted = fractal_submit_rlvr_proof(&mut pool, proof.proof_json).unwrap();
        assert_eq!(submitted.method, "fractal_submitRlvrProof");
        assert_eq!(submitted.proof_hash, proof.proof_hash);
        assert_eq!(submitted.pending_proofs, 1);
    }

    #[test]
    fn local_api_makes_rubrics_and_exports_loadable_adapter() {
        let dir = temp_dir("local-api-export");
        let _ = std::fs::remove_dir_all(&dir);
        let policy = RoutePolicy::default();
        let route_trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "What is the weather right now?",
                answer: Some("Use a weather lookup."),
                selected_route: "tiny-local-model",
                router_reason: "stable local route",
                route_policy: &policy,
                latency_ms: Some(1),
                cost_estimate: Some(0.0),
                user_rating: Some(5),
                user_correction: None,
            },
            "api-route-trace".into(),
            1_700_000_000,
            true,
        )
        .unwrap();
        let rubrics = fractal_make_rlvr_rubrics(MakeRubricsRequest {
            trace: route_trace,
            visible_prompt: Some("What is the weather right now?".into()),
            route_policy: policy,
            models: vec![ModelInventoryItem {
                model_id: "tiny-local-model".into(),
                local: true,
                capabilities: vec!["general".into()],
                max_cost: Some(0.0),
                max_latency_ms: Some(10),
            }],
            tools: vec![ToolInventoryItem {
                tool_id: "weather_lookup".into(),
                supports_current_info: true,
                safe_for_private_data: true,
            }],
        })
        .unwrap();
        assert_eq!(rubrics.method, "fractal_makeRlvrRubrics");
        assert_eq!(rubrics.rubrics.len(), 2);
        assert_eq!(rubrics.rubric_hashes.len(), 2);

        let export = fractal_export_rlvr_adapter(ExportAdapterRequest {
            adapter_id: "api-adapter".into(),
            base_model_id: "api-base-model".into(),
            out_dir: dir.join("adapter"),
            rank: Some(4),
            registry_path: None,
        })
        .unwrap();
        assert_eq!(export.method, "fractal_exportRlvrAdapter");
        assert_eq!(export.adapter_id, "api-adapter");
        assert!(export.loadable);
        assert!(std::path::Path::new(&export.manifest_path).exists());
        assert!(export.file_count >= 5);
    }

    #[test]
    fn submit_rlvr_proof_accepts_signed_json_and_inserts_into_pool() {
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();
        let bytes = serde_json::to_vec(&proof).unwrap();

        let response = fractal_submit_rlvr_proof(&mut pool, bytes).unwrap();

        assert_eq!(response.method, "fractal_submitRlvrProof");
        assert_eq!(response.proof_hash, proof.proof_hash().unwrap());
        assert_eq!(response.proof_type, RlvrProofType::ProofOfRoute);
        assert_eq!(response.node_id, "node-1");
        assert_eq!(response.pending_proofs, 1);
        assert!(pool.get(&response.proof_hash).is_some());
        assert_eq!(pool.metrics().proof_of_route_total, 1);
    }

    #[test]
    fn submit_rlvr_proof_accepts_canonical_proof_bytes() {
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();

        let response =
            fractal_submit_rlvr_proof(&mut pool, proof.canonical_bytes().unwrap()).unwrap();

        assert_eq!(response.proof_hash, proof.proof_hash().unwrap());
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn submit_rlvr_proof_rejects_invalid_signature_and_does_not_insert() {
        let mut pool = RlvrProofPool::new();
        let mut proof = signed_route_proof();
        proof.trace_hash = hash_bytes(b"tampered-trace");

        let err =
            fractal_submit_rlvr_proof(&mut pool, serde_json::to_vec(&proof).unwrap()).unwrap_err();

        assert!(err.to_string().contains("invalid node signature"));
        assert!(pool.is_empty());
    }

    #[test]
    fn submit_rlvr_proof_rejects_raw_private_fields_and_does_not_insert() {
        let mut pool = RlvrProofPool::new();
        let mut raw = serde_json::to_value(signed_route_proof()).unwrap();
        raw.as_object_mut()
            .unwrap()
            .insert("raw_prompt".into(), "My API key is sk-test-secret".into());

        let err =
            fractal_submit_rlvr_proof(&mut pool, serde_json::to_vec(&raw).unwrap()).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
        assert!(pool.is_empty());
    }

    #[test]
    fn submit_rlvr_proof_rejects_duplicate_proof_hash() {
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();

        let first =
            fractal_submit_rlvr_proof(&mut pool, serde_json::to_vec(&proof).unwrap()).unwrap();
        let err =
            fractal_submit_rlvr_proof(&mut pool, serde_json::to_vec(&proof).unwrap()).unwrap_err();

        assert!(err.to_string().contains(&first.proof_hash));
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.metrics().duplicate_total, 1);
    }

    fn committed_index_with(proof: &RlvrProofObject, height: u64) -> RlvrCommittedProofIndex {
        let mut index = RlvrCommittedProofIndex::new();
        let hash = proof.proof_hash().unwrap();
        index
            .insert(
                &hash,
                proof.clone(),
                RlvrProofBlockReference {
                    block_height: height,
                    block_hash: hash_bytes(format!("block-{height}").as_bytes()),
                },
                1_700_000_000_000,
            )
            .unwrap();
        index
    }

    #[test]
    fn get_rlvr_proof_reports_pending_status_for_a_pool_proof() {
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();
        let hash = pool.insert(proof).unwrap();
        let committed = RlvrCommittedProofIndex::new();

        let response = fractal_get_rlvr_proof(&pool, &committed, &hash).unwrap();

        assert_eq!(response.method, "fractal_getRlvrProof");
        assert!(response.found);
        assert_eq!(response.status, RlvrProofStatus::Pending);
        assert_eq!(response.proof_type, Some(RlvrProofType::ProofOfRoute));
        assert_eq!(response.timestamp_ms, Some(42));
        assert_eq!(response.node_id.as_deref(), Some("node-1"));
        assert!(response.block_reference.is_none());
    }

    #[test]
    fn get_rlvr_proof_reports_committed_status_with_block_reference() {
        let pool = RlvrProofPool::new();
        let proof = signed_route_proof();
        let hash = proof.proof_hash().unwrap();
        let committed = committed_index_with(&proof, 128);

        let response = fractal_get_rlvr_proof(&pool, &committed, &hash).unwrap();

        assert!(response.found);
        assert_eq!(response.status, RlvrProofStatus::Committed);
        let block = response.block_reference.unwrap();
        assert_eq!(block.block_height, 128);
        assert_eq!(block.block_hash, hash_bytes(b"block-128"));
    }

    #[test]
    fn get_rlvr_proof_reports_not_found_for_unknown_hash() {
        let pool = RlvrProofPool::new();
        let committed = RlvrCommittedProofIndex::new();
        let unknown = hash_bytes(b"never-submitted");

        let response = fractal_get_rlvr_proof(&pool, &committed, &unknown).unwrap();

        assert!(!response.found);
        assert_eq!(response.status, RlvrProofStatus::NotFound);
        assert!(response.proof_type.is_none());
        assert!(response.timestamp_ms.is_none());
        assert!(response.node_id.is_none());
        assert!(response.block_reference.is_none());
    }

    #[test]
    fn get_rlvr_proof_committed_status_wins_over_pending() {
        // Same proof present in both the pending pool and the committed index:
        // the committed block reference wins.
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();
        let hash = pool.insert(proof.clone()).unwrap();
        let committed = committed_index_with(&proof, 99);

        let response = fractal_get_rlvr_proof(&pool, &committed, &hash).unwrap();

        assert_eq!(response.status, RlvrProofStatus::Committed);
        assert_eq!(response.block_reference.unwrap().block_height, 99);
    }

    #[test]
    fn get_rlvr_proof_response_carries_no_raw_trace_data() {
        let mut pool = RlvrProofPool::new();
        let proof = signed_route_proof();
        let hash = pool.insert(proof).unwrap();
        let committed = RlvrCommittedProofIndex::new();

        let response = fractal_get_rlvr_proof(&pool, &committed, &hash).unwrap();
        let json = serde_json::to_string(&response).unwrap();

        // Status summary only — no raw trace fields, no proof body, no hashes
        // beyond the queried proof_hash and block hash.
        for forbidden in [
            "raw_prompt",
            "raw_answer",
            "trace_hash",
            "verifier_outputs_hash",
        ] {
            assert!(
                !json.contains(forbidden),
                "response leaked {forbidden}: {json}"
            );
        }
        assert!(json.contains("\"status\":\"pending\""));
        assert!(json.contains("\"proof_type\":\"ProofOfRoute\""));
    }

    #[test]
    fn get_rlvr_proof_rejects_malformed_hash() {
        let pool = RlvrProofPool::new();
        let committed = RlvrCommittedProofIndex::new();

        let err = fractal_get_rlvr_proof(&pool, &committed, "not-a-hash").unwrap_err();
        assert!(err.to_string().contains("proof_hash"));
    }

    fn signed_route_proof() -> RlvrProofObject {
        let key = NodeSigningKey::from_seed("node-1", b"test node seed").unwrap();
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfRoute,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "unsigned",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .sign_with_node_key(&key)
        .unwrap()
    }

    fn commitment_fixture() -> TraceHashCommitment {
        TraceHashCommitment {
            trace_id: "trace-1".into(),
            task_id: "task-1".into(),
            trace_hash: hash_bytes(b"trace"),
            redacted_trace_hash: hash_bytes(b"redacted"),
            verifier_outputs_hash: hash_bytes(b"verifier"),
            reward_vector_hash: hash_bytes(b"reward-vector"),
            privacy_tags: Vec::new(),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fractal-rlvr-{name}-{}", std::process::id()))
    }
}

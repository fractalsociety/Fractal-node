# Submitting RLMF Attestations from Fractalwork / DataEvol

Fractal-node exposes a JSON-RPC path for committing RLMF promotion attestations
on-chain. The 32-byte **canonical commitment** is mined as a native
`ProofCommitmentV1` transaction; the full record is indexed by the node and
queryable. Raw evidence stays off-chain by design.

## Endpoint

Inside the devnet compose network:

```
FRACTAL_CHAIN_RPC_URL=http://fractalchain-node:8545
```

(host-mapped: `http://127.0.0.1:18545`).

## Submit

```bash
curl -s -X POST -H 'Content-Type: application/json' "$FRACTAL_CHAIN_RPC_URL" -d '{
  "jsonrpc":"2.0","id":1,"method":"fractal_submitRlmfAttestation","params":[{
    "commitmentHash":   "0x…32-byte canonical commitment…",
    "subjectId":        "adapter.invoice_verifier.v7",
    "sourceSystem":     "fractalwork",
    "datasetHash":      "0x…", "jobHash": "0x…",
    "judgeReportHash":  "0x…", "benchmarkReportHash": "0x…",
    "modelArtifactHash":"0x…",
    "promotionDecision":"promote",
    "evidenceHashes":  ["0x…","0x…"],
    "lineageHashes":   ["0x…"]
  }]}'
```

Response: `{ network, transactionHash, blockNumber, finalized, attestation }`.

Rules: every hash field is a 0x-prefixed 32-byte hex string; `promotionDecision`
is one of `promote|reject|inconclusive|shadow|canary|rollback`; evidence/lineage
lists max 64 entries; unknown fields are rejected; resubmitting an *identical*
record is idempotent (returns the original inclusion); reusing a commitment with
different contents is an error.

## Canonical commitment (client-side computation)

`commitmentHash = keccak256( enc )` where `enc` is, in order:
`lp("rlmf.chain_attestation.v2") || lp(subjectId) || lp(sourceSystem) ||
datasetHash || jobHash || judgeReportHash || benchmarkReportHash ||
modelArtifactHash || lp(promotionDecision) ||
u32le(len(evidenceHashes)) || evidenceHashes… ||
u32le(len(lineageHashes)) || lineageHashes…`

with `lp(s) = u32le(len(s)) || utf8(s)` and hash fields as raw 32 bytes.
Reference implementation: `RlmfAttestationRecord::canonical_commitment` in
`crates/rpc/src/module.rs`. The node recomputes and rejects mismatches, so a
buggy client cannot commit a hash that doesn't cover its own record.

## Query

```bash
# by commitment
… -d '{"jsonrpc":"2.0","id":2,"method":"fractal_getRlmfAttestation",
       "params":[{"commitmentHash":"0x…"}]}'
# filtered list (subjectId / sourceSystem / blockNumber / transactionHash / limit)
… -d '{"jsonrpc":"2.0","id":3,"method":"fractal_listRlmfAttestations",
       "params":[{"sourceSystem":"dataevol","limit":20}]}'
```

## Durability note

The commitment itself is on-chain (survives anything the chain survives). The
indexed envelope is currently in node memory, matching `proof_commitments`
durability; snapshot/indexer persistence is tracked as task S3 in
`docs/STABILIZATION_TASK_LIST.md`.

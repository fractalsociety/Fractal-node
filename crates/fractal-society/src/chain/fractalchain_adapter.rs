//! FractalChain commitment adapter.
//!
//! The adapter is gated behind the `live-chain` feature. It builds the stable
//! JSON-RPC call shape for submitting proof hashes and delegates transport to a
//! caller-supplied RPC client, allowing CI to use deterministic mocks and live
//! deployments to provide a real jsonrpsee-backed client.

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::pkgs::chain_commitment::CommitmentAdapter;
use crate::protocol::{ChainReference, Hash};

/// JSON-RPC method used to submit a proof hash commitment.
pub const SUBMIT_PROOF_METHOD: &str = "fractal_submitProofHash";

/// Transport abstraction for FractalChain JSON-RPC calls.
pub trait FractalChainRpc: Send + Sync {
    /// Call `method` with JSON-RPC `params` and deserialize the response.
    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> crate::Result<FractalChainCommitmentResponse>;
}

/// Response returned by a FractalChain proof-commitment RPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FractalChainCommitmentResponse {
    /// Network name returned by the node.
    pub network: String,
    /// Transaction hash returned by the node.
    pub transaction_hash: String,
    /// Block number containing the commitment.
    pub block_number: u64,
    /// Whether the node considers the commitment finalized.
    pub finalized: bool,
}

impl From<FractalChainCommitmentResponse> for ChainReference {
    fn from(value: FractalChainCommitmentResponse) -> Self {
        Self {
            network: value.network,
            transaction_hash: value.transaction_hash,
            block_number: value.block_number,
            finalized: value.finalized,
        }
    }
}

/// FractalChain commitment adapter over an RPC transport.
#[derive(Debug, Clone)]
pub struct FractalChainCommitmentAdapter<T> {
    rpc: T,
}

impl<T> FractalChainCommitmentAdapter<T>
where
    T: FractalChainRpc,
{
    /// Create a new adapter from an RPC transport.
    pub fn new(rpc: T) -> Self {
        Self { rpc }
    }
}

impl<T> CommitmentAdapter for FractalChainCommitmentAdapter<T>
where
    T: FractalChainRpc,
{
    fn submit(&self, proof_hash: &Hash) -> crate::Result<ChainReference> {
        let response = self
            .rpc
            .call(SUBMIT_PROOF_METHOD, submit_params(proof_hash))?;
        Ok(response.into())
    }
}

/// Build JSON-RPC params for a proof-hash submission.
pub fn submit_params(proof_hash: &Hash) -> serde_json::Value {
    serde_json::json!([proof_hash.0])
}

/// jsonrpsee HTTP transport for a live FractalChain node.
pub struct JsonRpseeFractalChainRpc {
    client: jsonrpsee::http_client::HttpClient,
    runtime: tokio::runtime::Runtime,
}

impl JsonRpseeFractalChainRpc {
    /// Connect to a FractalChain JSON-RPC HTTP endpoint.
    pub fn connect(url: impl AsRef<str>) -> crate::Result<Self> {
        let client = jsonrpsee::http_client::HttpClientBuilder::default()
            .build(url.as_ref())
            .map_err(|err| {
                Error::ProtocolViolation(format!("failed to build FractalChain RPC client: {err}"))
            })?;
        Self::from_client(client)
    }

    /// Wrap an existing jsonrpsee HTTP client.
    pub fn from_client(client: jsonrpsee::http_client::HttpClient) -> crate::Result<Self> {
        let runtime = tokio::runtime::Runtime::new().map_err(Error::Io)?;
        Ok(Self { client, runtime })
    }
}

impl FractalChainRpc for JsonRpseeFractalChainRpc {
    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> crate::Result<FractalChainCommitmentResponse> {
        use jsonrpsee::core::client::ClientT;

        let params = to_rpc_params(params)?;
        self.runtime
            .block_on(self.client.request(method, params))
            .map_err(|err| Error::ProtocolViolation(format!("FractalChain RPC failed: {err}")))
    }
}

fn to_rpc_params(params: serde_json::Value) -> crate::Result<jsonrpsee::core::params::ArrayParams> {
    let serde_json::Value::Array(values) = params else {
        return Err(Error::ProtocolViolation(
            "FractalChain RPC params must be an array".to_string(),
        ));
    };

    let mut rpc_params = jsonrpsee::core::params::ArrayParams::new();
    for value in values {
        rpc_params.insert(value).map_err(Error::Json)?;
    }
    Ok(rpc_params)
}

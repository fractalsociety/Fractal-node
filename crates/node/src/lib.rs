//! Singleton dev node: 500 ms block cadence + JSON-RPC + libp2p/QUIC sync (`docs/prd.md` §18 M2).

pub mod p2p;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borsh::BorshDeserialize;
use fractal_consensus::{execute_and_build_block, header_hash, ordered_tx_root, Block, NATIVE_TX_GAS};
use fractal_core::{Address, State, Transaction};
use fractal_crypto::hash::keccak256;
use fractal_mempool::{next_base_fee, BaseFeeParams, Mempool, PooledTx};
use fractal_rpc::ChainInteraction;
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use thiserror::Error;
use tokio::sync::Mutex;

pub type NodeHandle = Arc<Mutex<NodeInner>>;

#[derive(Debug, Error)]
pub enum SyncApplyError {
    #[error("chain id mismatch")]
    ChainId,
    #[error("expected block height {expected}, got {got}")]
    Height { expected: u64, got: u64 },
    #[error("parent hash does not match local head")]
    ParentHash,
    #[error("state root mismatch after replay")]
    StateRoot,
    #[error("tx root mismatch after replay")]
    TxRoot,
    #[error(transparent)]
    Exec(#[from] fractal_core::ExecError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

const GENESIS_TAG: &[u8] = b"FRACTALCHAIN_GENESIS_V0";

pub fn genesis_parent_hash() -> fractal_crypto::Hash256 {
    keccak256(GENESIS_TAG)
}

pub struct NodeInner {
    pub chain_id: u64,
    pub height: u64,
    pub view: u64,
    pub head_hash: fractal_crypto::Hash256,
    pub parent_qc_hash: fractal_crypto::Hash256,
    pub proposer: fractal_crypto::Hash256,
    pub state: State,
    pub mempool: Mempool,
    pub base_fee: u128,
    pub gas_limit: u64,
    pub fee_params: BaseFeeParams,
    pub blocks: Vec<Block>,
}

impl NodeInner {
    pub fn devnet() -> Self {
        Self {
            chain_id: 41,
            height: 0,
            view: 0,
            head_hash: genesis_parent_hash(),
            parent_qc_hash: [0u8; 32],
            proposer: [0u8; 32],
            state: State::default(),
            mempool: Mempool::default(),
            base_fee: 1,
            gas_limit: 60_000_000,
            fee_params: BaseFeeParams::default(),
            blocks: Vec::new(),
        }
    }

    /// Replay txs and check roots against a received block (follower verification).
    pub fn apply_synced_block(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        if block.header.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        if block.header.height != self.height + 1 {
            return Err(SyncApplyError::Height {
                expected: self.height + 1,
                got: block.header.height,
            });
        }
        if block.header.parent_hash != self.head_hash {
            return Err(SyncApplyError::ParentHash);
        }
        let mut scratch = self.state.clone();
        fractal_core::apply_block(&mut scratch, &block.transactions)?;
        let sr = fractal_core::state_root(&scratch)?;
        if sr != block.header.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let tr = ordered_tx_root(&block.transactions)?;
        if tr != block.header.tx_root {
            return Err(SyncApplyError::TxRoot);
        }
        self.state = scratch;
        self.height = block.header.height;
        self.head_hash = header_hash(&block.header)?;
        self.view = block.header.view.wrapping_add(1);
        self.base_fee = next_base_fee(self.base_fee, block.header.gas_used, &self.fee_params);
        self.blocks.push(block.clone());
        Ok(())
    }
}

impl ChainInteraction for NodeInner {
    fn block_number(&self) -> u64 {
        self.height
    }

    fn balance_of(&self, addr: &Address) -> u128 {
        self.state.accounts.get(addr).map(|a| a.balance).unwrap_or(0)
    }

    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String> {
        let tx = Transaction::try_from_slice(raw).map_err(|e| format!("borsh decode: {e}"))?;
        self.mempool.insert(PooledTx {
            tx,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: u128::MAX,
        });
        Ok(())
    }

    fn base_fee_per_gas(&self) -> u128 {
        self.base_fee
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub async fn producer_loop(node: NodeHandle) {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let mut n = node.lock().await;
        let base = n.base_fee;
        let max_txs = (n.gas_limit / NATIVE_TX_GAS).max(1) as usize;
        let txs = n.mempool.drain_ready(max_txs, base);
        let parent = n.head_hash;
        let qc = n.parent_qc_hash;
        let height = n.height + 1;
        let view = n.view;
        let ts = now_ms();
        let chain_id = n.chain_id;
        let proposer = n.proposer;
        let gas_limit = n.gas_limit;
        match execute_and_build_block(
            chain_id,
            height,
            view,
            parent,
            qc,
            proposer,
            ts,
            gas_limit,
            &mut n.state,
            txs,
        ) {
            Ok(block) => {
                if let Ok(hh) = header_hash(&block.header) {
                    n.head_hash = hh;
                }
                n.height = block.header.height;
                n.view = n.view.wrapping_add(1);
                n.base_fee = next_base_fee(n.base_fee, block.header.gas_used, &n.fee_params);
                n.blocks.push(block);
            }
            Err(e) => eprintln!("fractal-node: block execution failed: {e}"),
        }
    }
}

pub async fn run_dev() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let node: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8545".into())
        .parse()?;
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone()).await?;
    eprintln!("fractal-node json-rpc at http://{bound}");

    let listen: Multiaddr = std::env::var("FRACTAL_P2P_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/udp/4001/quic-v1".into())
        .parse()?;
    let (tx_ready, rx_ready) = tokio::sync::oneshot::channel();
    let p2p_node = node.clone();
    tokio::spawn(async move {
        if let Err(e) = p2p::producer_network_task(p2p_node, listen, Some(tx_ready)).await {
            eprintln!("fractal-node p2p: {e}");
        }
    });
    match tokio::time::timeout(Duration::from_secs(8), rx_ready).await {
        Ok(Ok((bound_p2p, peer))) => {
            let mut bootstrap = bound_p2p.clone();
            bootstrap.push(Protocol::P2p(peer));
            eprintln!("fractal-node p2p (QUIC) listening {bound_p2p}; follower env FRACTAL_BOOTSTRAP={bootstrap}");
        }
        Ok(Err(_)) => eprintln!("fractal-node p2p: ready channel dropped"),
        Err(_) => eprintln!("fractal-node p2p: timed out waiting for listen address"),
    }

    tokio::spawn(producer_loop(node));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

/// Follower: JSON-RPC + sync from `FRACTAL_BOOTSTRAP` (multiaddr with `/p2p/<PeerId>`).
pub async fn run_follower() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bootstrap: Multiaddr = std::env::var("FRACTAL_BOOTSTRAP")?.parse()?;
    let node: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8546".into())
        .parse()?;
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone()).await?;
    eprintln!("fractal-node follower json-rpc at http://{bound}");
    tokio::spawn(p2p::follower_network_task(node, bootstrap));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

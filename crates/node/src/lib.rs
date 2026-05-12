//! Singleton dev node: 500 ms block cadence + JSON-RPC + libp2p/QUIC sync (`docs/prd.md` §18 M2).

pub mod p2p;
mod eth_signed;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borsh::BorshDeserialize;
use fractal_consensus::{execute_and_build_block, header_hash, ordered_tx_root, Block};
use fractal_core::{Address, EvmEngine, State, Transaction};
use fractal_crypto::hash::keccak256;
use fractal_mempool::{next_base_fee, BaseFeeParams, Mempool, PooledTx};
use fractal_rpc::{make_rpc_log, logs_bloom_256, ChainInteraction};
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
    #[error("gas used mismatch: header {header}, replay {replay}")]
    GasUsedMismatch { header: u64, replay: u64 },
    #[error(transparent)]
    Exec(#[from] fractal_core::ExecError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

const GENESIS_TAG: &[u8] = b"FRACTALCHAIN_GENESIS_V0";

/// Address for Hardhat / Anvil **default signer #0** (`0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`).
/// Prefunded in [`NodeInner::devnet`] so `contracts/` Hardhat deploy works without extra setup.
pub const HARDHAT_DEFAULT_SIGNER_0: Address = [
    0xf3, 0x9F, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xF6, 0xF4, 0xce, 0x6a, 0xB8, 0x82, 0x72, 0x79, 0xcf, 0xfF, 0xb9, 0x22, 0x66,
];

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
    pub pending_txs: BTreeMap<fractal_crypto::Hash256, Transaction>,
    pub mined_txs: BTreeMap<fractal_crypto::Hash256, (u64, fractal_crypto::Hash256, u32)>,
    /// Signed EIP-1559 bytes keyed by `keccak256(raw)` (RPC tx hash).
    pub eth_signed_raw: BTreeMap<fractal_crypto::Hash256, Vec<u8>>,
    /// When RPC hash differs from `keccak(borsh(tx))` (EVM state keys), map RPC → internal.
    pub eth_rpc_to_internal_tx_hash: BTreeMap<fractal_crypto::Hash256, fractal_crypto::Hash256>,
    /// Inverse of the above for log `transactionHash` fields (`eth_getLogs`).
    pub eth_internal_to_rpc_tx_hash: BTreeMap<fractal_crypto::Hash256, fractal_crypto::Hash256>,
}

impl NodeInner {
    pub fn devnet() -> Self {
        let mut state = State::default();
        state.accounts.insert(
            HARDHAT_DEFAULT_SIGNER_0,
            fractal_core::Account {
                nonce: 0,
                balance: 1_000_000_000_000_000_000_000_000u128,
            },
        );
        Self {
            chain_id: 41,
            height: 0,
            view: 0,
            head_hash: genesis_parent_hash(),
            parent_qc_hash: [0u8; 32],
            proposer: [0u8; 32],
            state,
            mempool: Mempool::default(),
            base_fee: 1,
            gas_limit: 60_000_000,
            fee_params: BaseFeeParams::default(),
            blocks: Vec::new(),
            pending_txs: BTreeMap::new(),
            mined_txs: BTreeMap::new(),
            eth_signed_raw: BTreeMap::new(),
            eth_rpc_to_internal_tx_hash: BTreeMap::new(),
            eth_internal_to_rpc_tx_hash: BTreeMap::new(),
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
        let mut evm = fractal_evm::RevmEngine::default();
        let gas = fractal_core::apply_block_with_evm(&mut scratch, &block.transactions, &mut evm)?;
        if gas != block.header.gas_used {
            return Err(SyncApplyError::GasUsedMismatch {
                header: block.header.gas_used,
                replay: gas,
            });
        }
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

    /// Sum of log counts for transactions before `tx_index` in `block_number`.
    fn log_index_base_in_block(&self, block_number: u64, tx_index: u32) -> u64 {
        let Some(idx) = block_number.checked_sub(1).map(|x| x as usize) else {
            return 0;
        };
        let Some(block) = self.blocks.get(idx) else {
            return 0;
        };
        let mut n = 0u64;
        for (i, tx) in block.transactions.iter().enumerate() {
            if i >= tx_index as usize {
                break;
            }
            let Ok(raw) = borsh::to_vec(tx) else {
                continue;
            };
            let th = keccak256(&raw);
            if let Some(ls) = self.state.evm_tx_logs.get(&th) {
                n += ls.len() as u64;
            }
        }
        n
    }

    fn internal_tx_hash_for_state(&self, rpc_hash: &[u8; 32]) -> fractal_crypto::Hash256 {
        self.eth_rpc_to_internal_tx_hash
            .get(rpc_hash)
            .copied()
            .unwrap_or(*rpc_hash)
    }
}

impl ChainInteraction for NodeInner {
    fn block_number(&self) -> u64 {
        self.height
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn balance_of(&self, addr: &Address) -> u128 {
        self.state.accounts.get(addr).map(|a| a.balance).unwrap_or(0)
    }

    fn transaction_count(&self, addr: &Address) -> u64 {
        self.state.accounts.get(addr).map(|a| a.nonce).unwrap_or(0)
    }

    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String> {
        // Dev stub: accept either (a) borsh-encoded internal txs, or (b) real Ethereum EIP-1559
        // signed tx bytes (type 0x02) for Hardhat/MetaMask compatibility.
        if let Ok(tx) = Transaction::try_from_slice(raw) {
            let h = keccak256(raw);
            self.pending_txs.insert(h, tx.clone());
            self.mempool.insert(PooledTx {
                tx,
                max_priority_fee_per_gas: 1,
                max_fee_per_gas: u128::MAX,
                eth_signed_raw: None,
            });
            return Ok(());
        }

        let (tx, h, max_priority_fee_per_gas, max_fee_per_gas) =
            eth_signed::to_core_tx(raw, self.chain_id)?;
        self.pending_txs.insert(h, tx.clone());
        self.eth_signed_raw.insert(h, raw.to_vec());
        self.mempool.insert(PooledTx {
            tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            eth_signed_raw: Some(raw.to_vec()),
        });
        Ok(())
    }

    fn base_fee_per_gas(&self) -> u128 {
        self.base_fee
    }

    fn block_hash_by_number(&self, number: u64) -> Option<[u8; 32]> {
        if number == 0 {
            return Some(genesis_parent_hash());
        }
        let idx = number.checked_sub(1)? as usize;
        let b = self.blocks.get(idx)?;
        header_hash(&b.header).ok()
    }

    fn block_by_hash(&self, hash: &[u8; 32]) -> Option<Block> {
        if hash == &genesis_parent_hash() {
            // Synthetic genesis block: minimal header-like object.
            return Some(Block {
                header: fractal_consensus::BlockHeader {
                    version: 1,
                    chain_id: self.chain_id,
                    height: 0,
                    view: 0,
                    parent_hash: [0u8; 32],
                    parent_qc_hash: [0u8; 32],
                    proposer: [0u8; 32],
                    timestamp_ms: 0,
                    state_root: [0u8; 32],
                    tx_root: [0u8; 32],
                    gas_used: 0,
                    gas_limit: self.gas_limit,
                    extra: [0u8; 32],
                },
                transactions: Vec::new(),
            });
        }
        self.blocks
            .iter()
            .find(|b| header_hash(&b.header).ok().as_ref() == Some(hash))
            .cloned()
    }

    fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<Transaction> {
        if let Some(tx) = self.pending_txs.get(hash) {
            return Some(tx.clone());
        }
        if let Some((bn, _bh, idx)) = self.mined_txs.get(hash) {
            if *bn == 0 {
                return None;
            }
            let bi = (*bn as usize).checked_sub(1)?;
            let block = self.blocks.get(bi)?;
            return block.transactions.get(*idx as usize).cloned();
        }
        for b in &self.blocks {
            for tx in &b.transactions {
                let raw = borsh::to_vec(tx).ok()?;
                if &keccak256(&raw) == hash {
                    return Some(tx.clone());
                }
            }
        }
        None
    }

    fn mined_tx_info(&self, hash: &[u8; 32]) -> Option<(u64, [u8; 32], u32)> {
        self.mined_txs.get(hash).cloned()
    }

    fn eth_signed_raw(&self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.eth_signed_raw.get(tx_hash).cloned()
    }

    fn simulate_eth_call(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, fractal_core::ExecError> {
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        match to {
            Some(to) => evm
                .execute_call(&mut scratch, from, to, value, data, self.gas_limit)
                .map(|o| o.return_data),
            None => evm
                .execute_create(&mut scratch, from, value, data, self.gas_limit)
                .map(|o| o.return_data),
        }
    }

    fn estimate_eth_gas(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<u64, fractal_core::ExecError> {
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        match to {
            Some(to) => evm
                .execute_call(&mut scratch, from, to, value, data, self.gas_limit)
                .map(|o| o.gas_used),
            None => evm
                .execute_create(&mut scratch, from, value, data, self.gas_limit)
                .map(|o| o.gas_used),
        }
    }

    fn code_at(&self, addr: &Address) -> Vec<u8> {
        self.state.evm_code.get(addr).cloned().unwrap_or_default()
    }

    fn storage_at(&self, addr: &Address, slot: [u8; 32]) -> [u8; 32] {
        self.state
            .evm_storage
            .get(&(*addr, slot))
            .copied()
            .unwrap_or([0u8; 32])
    }

    fn gas_used_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u64> {
        let k = self.internal_tx_hash_for_state(tx_hash);
        self.state.evm_tx_gas_used.get(&k).copied()
    }

    fn evm_receipt_success(&self, tx_hash: &[u8; 32]) -> bool {
        let k = self.internal_tx_hash_for_state(tx_hash);
        self.state
            .evm_tx_success
            .get(&k)
            .copied()
            .unwrap_or(true)
    }

    fn logs_for_filter(&self, filter: &fractal_rpc::LogsFilter) -> Vec<fractal_rpc::RpcLog> {
        let mut out = Vec::new();
        let start = filter.from_block.max(1);
        let end = filter.to_block.max(1);

        for height in start..=end {
            let idx = match height.checked_sub(1) {
                Some(i) => i as usize,
                None => continue,
            };
            let Some(block) = self.blocks.get(idx) else { continue };
            let bh = match header_hash(&block.header) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let mut block_log_index: u64 = 0;
            for (txi, tx) in block.transactions.iter().enumerate() {
                let raw = match borsh::to_vec(tx) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let th = keccak256(&raw);
                let rpc_h = self
                    .eth_internal_to_rpc_tx_hash
                    .get(&th)
                    .copied()
                    .unwrap_or(th);
                let Some(logs) = self.state.evm_tx_logs.get(&th) else { continue };
                for l in logs {
                    if let Some(ref addrs) = filter.addresses {
                        if !addrs.contains(&l.address) {
                            continue;
                        }
                    }
                    if !fractal_rpc::evm_log_matches_topic_filters(l, &filter.topic_filters) {
                        continue;
                    }
                    out.push(make_rpc_log(
                        l,
                        &bh,
                        height,
                        &rpc_h,
                        txi as u32,
                        block_log_index,
                    ));
                    block_log_index += 1;
                }
            }
        }
        out
    }

    fn receipt_rpc_logs(
        &self,
        tx_hash: &[u8; 32],
        block_number: u64,
        block_hash: &[u8; 32],
        tx_index: u32,
    ) -> (Vec<fractal_rpc::RpcLog>, [u8; 256]) {
        let k = self.internal_tx_hash_for_state(tx_hash);
        let Some(evm_logs) = self.state.evm_tx_logs.get(&k) else {
            return (Vec::new(), [0u8; 256]);
        };
        let bloom = logs_bloom_256(evm_logs);
        let start = self.log_index_base_in_block(block_number, tx_index);
        let rpc_logs = evm_logs
            .iter()
            .enumerate()
            .map(|(i, l)| make_rpc_log(l, block_hash, block_number, tx_hash, tx_index, start + i as u64))
            .collect();
        (rpc_logs, bloom)
    }

    fn logs_bloom_for_block(&self, block: &Block) -> [u8; 256] {
        let mut acc = [0u8; 256];
        for tx in &block.transactions {
            let Ok(raw) = borsh::to_vec(tx) else {
                continue;
            };
            let th = keccak256(&raw);
            let Some(logs) = self.state.evm_tx_logs.get(&th) else {
                continue;
            };
            let b = logs_bloom_256(logs);
            for i in 0..256 {
                acc[i] |= b[i];
            }
        }
        acc
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
        let gas_limit_cfg = n.gas_limit;
        let pooled = n.mempool.drain_ready_gas_budget(gas_limit_cfg, base);
        let eth_raws: Vec<Option<Vec<u8>>> = pooled.iter().map(|p| p.eth_signed_raw.clone()).collect();
        let txs: Vec<Transaction> = pooled.into_iter().map(|p| p.tx).collect();
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
                let hh = header_hash(&block.header).unwrap_or([0u8; 32]);
                n.head_hash = hh;
                n.height = block.header.height;
                n.view = n.view.wrapping_add(1);
                n.base_fee = next_base_fee(n.base_fee, block.header.gas_used, &n.fee_params);
                // Mark txs as mined for RPC.
                for (i, tx) in block.transactions.iter().enumerate() {
                    let Ok(borsh_raw) = borsh::to_vec(tx) else {
                        continue;
                    };
                    let ih = keccak256(&borsh_raw);
                    let rpc_h = if let Some(Some(eth_raw)) = eth_raws.get(i) {
                        let eh = keccak256(eth_raw);
                        if eh != ih {
                            n.eth_rpc_to_internal_tx_hash.insert(eh, ih);
                            n.eth_internal_to_rpc_tx_hash.insert(ih, eh);
                        }
                        n.eth_signed_raw.insert(eh, eth_raw.clone());
                        eh
                    } else {
                        ih
                    };
                    n.pending_txs.remove(&rpc_h);
                    n.mined_txs
                        .insert(rpc_h, (block.header.height, hh, i as u32));
                }
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

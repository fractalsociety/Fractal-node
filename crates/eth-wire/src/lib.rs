//! EIP-1559 RLP wire helpers shared by `fractal-node` and `fractal-rpc`.

use fractal_core::{Address, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use rlp::{Rlp, RlpStream};

#[derive(Debug, Clone)]
pub struct Eth1559Envelope {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub to: Option<Address>,
    pub value: u128,
    pub data: Vec<u8>,
    pub y_parity: u8,
    pub r: [u8; 32],
    pub s: [u8; 32],
}

fn rlp_u64(v: &Rlp) -> Result<u64, String> {
    v.as_val::<u64>().map_err(|e| format!("rlp u64: {e}"))
}

fn rlp_u128(v: &Rlp) -> Result<u128, String> {
    v.as_val::<u128>().map_err(|e| format!("rlp u128: {e}"))
}

fn rlp_bytes(v: &Rlp) -> Result<Vec<u8>, String> {
    v.data()
        .map(|d| d.to_vec())
        .map_err(|e| format!("rlp bytes: {e}"))
}

fn rlp_h256(v: &Rlp) -> Result<[u8; 32], String> {
    let b = v.data().map_err(|e| format!("rlp bytes32: {e}"))?;
    if b.len() > 32 {
        return Err("rlp bytes32: too long".into());
    }
    let mut out = [0u8; 32];
    out[32 - b.len()..].copy_from_slice(b);
    Ok(out)
}

fn rlp_to(v: &Rlp) -> Result<Option<Address>, String> {
    let b = v.data().map_err(|e| format!("rlp to: {e}"))?;
    if b.is_empty() {
        return Ok(None);
    }
    if b.len() != 20 {
        return Err("rlp to: expected 0 or 20 bytes".into());
    }
    let mut a = [0u8; 20];
    a.copy_from_slice(b);
    Ok(Some(a))
}

pub fn decode_eip1559(raw: &[u8]) -> Result<Eth1559Envelope, String> {
    if raw.first().copied() != Some(0x02) {
        return Err("not an EIP-1559 (0x02) transaction".into());
    }
    let rlp = Rlp::new(&raw[1..]);
    if !rlp.is_list() {
        return Err("invalid rlp: expected list".into());
    }
    if rlp.item_count().map_err(|e| format!("rlp item_count: {e}"))? != 12 {
        return Err("invalid rlp: expected 12 fields for 1559 tx".into());
    }

    let chain_id = rlp_u64(&rlp.at(0).map_err(|e| format!("rlp at 0: {e}"))?)?;
    let nonce = rlp_u64(&rlp.at(1).map_err(|e| format!("rlp at 1: {e}"))?)?;
    let max_priority_fee_per_gas = rlp_u128(&rlp.at(2).map_err(|e| format!("rlp at 2: {e}"))?)?;
    let max_fee_per_gas = rlp_u128(&rlp.at(3).map_err(|e| format!("rlp at 3: {e}"))?)?;
    let gas_limit = rlp_u64(&rlp.at(4).map_err(|e| format!("rlp at 4: {e}"))?)?;
    let to = rlp_to(&rlp.at(5).map_err(|e| format!("rlp at 5: {e}"))?)?;
    let value = rlp_u128(&rlp.at(6).map_err(|e| format!("rlp at 6: {e}"))?)?;
    let data = rlp_bytes(&rlp.at(7).map_err(|e| format!("rlp at 7: {e}"))?)?;

    let access_list_raw = rlp
        .at(8)
        .map_err(|e| format!("rlp at 8: {e}"))?
        .as_raw()
        .to_vec();

    let y_parity = rlp
        .at(9)
        .map_err(|e| format!("rlp at 9: {e}"))?
        .as_val::<u8>()
        .map_err(|e| format!("rlp y_parity: {e}"))?;
    let r = rlp_h256(&rlp.at(10).map_err(|e| format!("rlp at 10: {e}"))?)?;
    let s = rlp_h256(&rlp.at(11).map_err(|e| format!("rlp at 11: {e}"))?)?;

    let env = Eth1559Envelope {
        chain_id,
        nonce,
        max_priority_fee_per_gas,
        max_fee_per_gas,
        gas_limit,
        to,
        value,
        data,
        y_parity,
        r,
        s,
    };
    let _ = Rlp::new(&access_list_raw);
    Ok(env)
}

fn eip1559_sighash(env: &Eth1559Envelope, access_list_rlp: &[u8]) -> [u8; 32] {
    let mut s = RlpStream::new_list(9);
    s.append(&env.chain_id);
    s.append(&env.nonce);
    s.append(&env.max_priority_fee_per_gas);
    s.append(&env.max_fee_per_gas);
    s.append(&env.gas_limit);
    match &env.to {
        Some(to) => s.append(&to.as_slice()),
        None => s.append_empty_data(),
    };
    s.append(&env.value);
    s.append(&env.data.as_slice());
    s.append_raw(access_list_rlp, 1);

    let out = s.out();
    let mut prefixed = Vec::with_capacity(1 + out.len());
    prefixed.push(0x02);
    prefixed.extend_from_slice(&out);
    keccak256(&prefixed)
}

pub fn recover_sender_eip1559(raw: &[u8], env: &Eth1559Envelope) -> Result<Address, String> {
    let rlp = Rlp::new(&raw[1..]);
    let access_list_rlp = rlp.at(8).map_err(|e| format!("rlp at 8: {e}"))?.as_raw();
    let sighash = eip1559_sighash(env, access_list_rlp);

    if env.y_parity > 1 {
        return Err("bad yParity (expected 0 or 1)".into());
    }
    let recid = RecoveryId::try_from(env.y_parity).map_err(|e| format!("recovery id: {e}"))?;
    let sig = Signature::from_scalars(env.r, env.s).map_err(|e| format!("sig scalars: {e}"))?;
    let vk = VerifyingKey::recover_from_prehash(&sighash, &sig, recid).map_err(|e| format!("recover: {e}"))?;
    let uncompressed = vk.to_encoded_point(false);
    let pubkey = uncompressed.as_bytes();
    if pubkey.len() != 65 || pubkey[0] != 0x04 {
        return Err("unexpected pubkey encoding".into());
    }
    let h = keccak256(&pubkey[1..]);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&h[12..]);
    Ok(addr)
}

pub fn to_core_tx(raw: &[u8], expected_chain_id: u64) -> Result<(Transaction, [u8; 32], u128, u128), String> {
    let env = decode_eip1559(raw)?;
    if env.chain_id != expected_chain_id {
        return Err(format!("wrong chainId: got {}, expected {}", env.chain_id, expected_chain_id));
    }
    let signer = recover_sender_eip1559(raw, &env)?;
    let tx_hash = keccak256(raw);

    let body = match env.to {
        Some(to) => {
            if env.data.is_empty() {
                TxBody::Transfer { to, amount: env.value }
            } else {
                TxBody::EvmCall {
                    to,
                    value: env.value,
                    calldata: env.data,
                    gas_limit: env.gas_limit,
                }
            }
        }
        None => TxBody::EvmCreate {
            value: env.value,
            init_code: env.data,
            gas_limit: env.gas_limit,
        },
    };

    Ok((
        Transaction {
            signer,
            nonce: env.nonce,
            vm: VmKind::Evm,
            body,
        },
        tx_hash,
        env.max_priority_fee_per_gas,
        env.max_fee_per_gas,
    ))
}

fn h32(v: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(v))
}

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn q_u64(v: u64) -> String {
    format!("0x{:x}", v)
}

fn q_u128(v: u128) -> String {
    format!("0x{:x}", v)
}

/// JSON-RPC `eth_getTransactionByHash` object for a signed EIP-1559 payload (ethers.js / Hardhat).
pub fn eip1559_signed_tx_to_json(
    raw: &[u8],
    mined: Option<(u64, [u8; 32], u32)>,
) -> Result<serde_json::Value, String> {
    let env = decode_eip1559(raw)?;
    let from = recover_sender_eip1559(raw, &env)?;
    let hash = keccak256(raw);

    let to_hex = env.to.as_ref().map(addr_hex);

    let (block_hash, block_number, tx_index) = match mined {
        Some((bn, bh, idx)) => (
            Some(format!("0x{}", hex::encode(bh))),
            Some(q_u64(bn)),
            Some(q_u64(idx as u64)),
        ),
        None => (None, None, None),
    };

    Ok(serde_json::json!({
        "hash": format!("0x{}", hex::encode(hash)),
        "type": "0x2",
        "chainId": q_u64(env.chain_id),
        "nonce": q_u64(env.nonce),
        "from": addr_hex(&from),
        "to": to_hex,
        "value": q_u128(env.value),
        "gas": q_u64(env.gas_limit),
        "gasPrice": q_u128(env.max_fee_per_gas),
        "maxFeePerGas": q_u128(env.max_fee_per_gas),
        "maxPriorityFeePerGas": q_u128(env.max_priority_fee_per_gas),
        "input": format!("0x{}", hex::encode(&env.data)),
        "accessList": [],
        "yParity": q_u64(env.y_parity as u64),
        "r": h32(&env.r),
        "s": h32(&env.s),
        "blockHash": block_hash,
        "blockNumber": block_number,
        "transactionIndex": tx_index,
    }))
}

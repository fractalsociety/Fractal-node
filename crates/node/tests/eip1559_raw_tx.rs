use k256::ecdsa::{signature::hazmat::PrehashSigner, RecoveryId, Signature, SigningKey, VerifyingKey};
use rlp::RlpStream;

use std::sync::Arc;

use fractal_core::{TxBody, VmKind, WEI_PER_FRAC};
use fractal_crypto::hash::keccak256;
use fractal_eth_wire::eip1559_signed_tx_to_json;
use fractal_node::{try_produce_one_tick, NodeInner, ProduceTickOutcome};
use fractal_rpc::ChainInteraction;
use tokio::sync::Mutex;

fn addr_from_vk(vk: &VerifyingKey) -> [u8; 20] {
    let uncompressed = vk.to_encoded_point(false);
    let pubkey = uncompressed.as_bytes();
    let h = keccak256(&pubkey[1..]);
    let mut a = [0u8; 20];
    a.copy_from_slice(&h[12..]);
    a
}

fn addr_hex(addr: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(addr))
}

fn build_and_sign_1559(
    chain_id: u64,
    nonce: u64,
    max_priority: u128,
    max_fee: u128,
    gas_limit: u64,
    to: Option<[u8; 20]>,
    value: u128,
    data: Vec<u8>,
    sk: &SigningKey,
) -> Vec<u8> {
    // access list = []
    let access_list = RlpStream::new_list(0);
    let access_list_rlp = access_list.out();

    // sighash rlp (9 items)
    let mut s = RlpStream::new_list(9);
    s.append(&chain_id);
    s.append(&nonce);
    s.append(&max_priority);
    s.append(&max_fee);
    s.append(&gas_limit);
    match &to {
        Some(to) => s.append(&to.as_slice()),
        None => s.append_empty_data(),
    };
    s.append(&value);
    s.append(&data.as_slice());
    s.append_raw(&access_list_rlp, 1);
    let out = s.out();
    let mut preimage = Vec::with_capacity(1 + out.len());
    preimage.push(0x02);
    preimage.extend_from_slice(&out);
    let sighash = keccak256(&preimage);

    let sig: Signature = sk.sign_prehash(&sighash).unwrap();
    // brute-force recovery id (0 or 1) and pick the one that matches pubkey
    let vk = sk.verifying_key();
    let mut y_parity = 0u8;
    for cand in [0u8, 1u8] {
        let rid = RecoveryId::try_from(cand).unwrap();
        let rec = VerifyingKey::recover_from_prehash(&sighash, &sig, rid).unwrap();
        if &rec == vk {
            y_parity = cand;
            break;
        }
    }

    let (r, s) = (sig.r().to_bytes(), sig.s().to_bytes());

    // full tx rlp (12 items)
    let mut tx = RlpStream::new_list(12);
    tx.append(&chain_id);
    tx.append(&nonce);
    tx.append(&max_priority);
    tx.append(&max_fee);
    tx.append(&gas_limit);
    match &to {
        Some(to) => tx.append(&to.as_slice()),
        None => tx.append_empty_data(),
    };
    tx.append(&value);
    tx.append(&data.as_slice());
    tx.append_raw(&access_list_rlp, 1);
    tx.append(&y_parity);
    tx.append(&r.as_slice());
    tx.append(&s.as_slice());
    let tx_rlp = tx.out();

    let mut raw = Vec::with_capacity(1 + tx_rlp.len());
    raw.push(0x02);
    raw.extend_from_slice(&tx_rlp);
    raw
}

#[test]
fn node_accepts_eip1559_raw_tx_and_recovers_sender() {
    let mut node = NodeInner::devnet();

    let sk = SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
    let from = addr_from_vk(sk.verifying_key());
    node.state.accounts.insert(from, fractal_core::Account { nonce: 0, balance: 10_000_000 });

    let to = [0x11u8; 20];
    let raw = build_and_sign_1559(node.chain_id, 0, 1, 10, 21_000, Some(to), 123, vec![], &sk);
    let tx_hash = node.submit_raw_tx(&raw).expect("accept raw eth tx");
    let h = keccak256(&raw);
    assert_eq!(tx_hash, h);
    let stored = node.pending_txs.get(&h).expect("pending tx stored");
    assert_eq!(stored.signer, from);
    assert_eq!(stored.vm, VmKind::Evm);
    match &stored.body {
        TxBody::Transfer { to: dst, amount } => {
            assert_eq!(*dst, to);
            assert_eq!(*amount, 123);
        }
        _ => panic!("expected transfer tx"),
    }
}

#[test]
fn pending_nonce_counts_queued_metamask_transfers() {
    let mut node = NodeInner::devnet();

    let sk = SigningKey::from_bytes(&[8u8; 32].into()).unwrap();
    let from = addr_from_vk(sk.verifying_key());
    node.state.accounts.insert(
        from,
        fractal_core::Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );

    let to = [0x12u8; 20];
    let raw0 = build_and_sign_1559(node.chain_id, 0, 1, 10, 21_000, Some(to), 123, vec![], &sk);
    node.submit_raw_tx(&raw0).expect("accept first raw eth tx");
    assert_eq!(node.transaction_count(&from), 0);
    assert_eq!(node.pending_transaction_count(&from), 1);

    let raw1 = build_and_sign_1559(node.chain_id, 1, 1, 10, 21_000, Some(to), 456, vec![], &sk);
    node.submit_raw_tx(&raw1).expect("accept second raw eth tx");
    assert_eq!(node.pending_transaction_count(&from), 2);
}

#[tokio::test]
async fn metamask_style_transfers_move_10_tokens_from_two_different_wallets() {
    let mut node = NodeInner::devnet();

    let sk_a = SigningKey::from_bytes(&[0x21u8; 32].into()).unwrap();
    let from_a = addr_from_vk(sk_a.verifying_key());
    let to_a = [0xaau8; 20];

    let sk_b = SigningKey::from_bytes(&[0x22u8; 32].into()).unwrap();
    let from_b = addr_from_vk(sk_b.verifying_key());
    let to_b = [0xbbu8; 20];

    let starting_balance = 100 * WEI_PER_FRAC;
    let transfer_amount = 10 * WEI_PER_FRAC;
    println!(
        "10-token transfer A: {} -> {}",
        addr_hex(&from_a),
        addr_hex(&to_a)
    );
    println!(
        "10-token transfer B: {} -> {}",
        addr_hex(&from_b),
        addr_hex(&to_b)
    );
    node.state.accounts.insert(
        from_a,
        fractal_core::Account {
            nonce: 0,
            balance: starting_balance,
        },
    );
    node.state.accounts.insert(
        from_b,
        fractal_core::Account {
            nonce: 0,
            balance: starting_balance,
        },
    );

    let raw_a = build_and_sign_1559(
        node.chain_id,
        0,
        1,
        10,
        21_000,
        Some(to_a),
        transfer_amount,
        vec![],
        &sk_a,
    );
    let hash_a = node
        .submit_raw_tx(&raw_a)
        .expect("accept first MetaMask-style transfer");
    assert_eq!(hash_a, keccak256(&raw_a));

    let raw_b = build_and_sign_1559(
        node.chain_id,
        0,
        1,
        10,
        21_000,
        Some(to_b),
        transfer_amount,
        vec![],
        &sk_b,
    );
    let hash_b = node
        .submit_raw_tx(&raw_b)
        .expect("accept second MetaMask-style transfer");
    assert_eq!(hash_b, keccak256(&raw_b));

    assert_eq!(node.pending_transaction_count(&from_a), 1);
    assert_eq!(node.pending_transaction_count(&from_b), 1);

    let node = Arc::new(Mutex::new(node));
    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    );

    let node = node.lock().await;
    assert_eq!(node.balance_of(&from_a), starting_balance - transfer_amount);
    assert_eq!(node.balance_of(&to_a), transfer_amount);
    assert_eq!(node.transaction_count(&from_a), 1);

    assert_eq!(node.balance_of(&from_b), starting_balance - transfer_amount);
    assert_eq!(node.balance_of(&to_b), transfer_amount);
    assert_eq!(node.transaction_count(&from_b), 1);

    assert!(node.mined_tx_info(&hash_a).is_some());
    assert!(node.mined_tx_info(&hash_b).is_some());
    assert_eq!(node.eth_signed_raw(&hash_a).as_deref(), Some(raw_a.as_slice()));
    assert_eq!(node.eth_signed_raw(&hash_b).as_deref(), Some(raw_b.as_slice()));
}

#[tokio::test]
async fn metamask_style_transfers_move_37_tokens_from_two_more_wallets() {
    let mut node = NodeInner::devnet();

    let sk_a = SigningKey::from_bytes(&[0x31u8; 32].into()).unwrap();
    let from_a = addr_from_vk(sk_a.verifying_key());
    let to_a = [0xceu8; 20];

    let sk_b = SigningKey::from_bytes(&[0x32u8; 32].into()).unwrap();
    let from_b = addr_from_vk(sk_b.verifying_key());
    let to_b = [0xdfu8; 20];

    let starting_balance = 250 * WEI_PER_FRAC;
    let transfer_amount = 37 * WEI_PER_FRAC;
    println!(
        "37-token transfer A: {} -> {}",
        addr_hex(&from_a),
        addr_hex(&to_a)
    );
    println!(
        "37-token transfer B: {} -> {}",
        addr_hex(&from_b),
        addr_hex(&to_b)
    );
    node.state.accounts.insert(
        from_a,
        fractal_core::Account {
            nonce: 0,
            balance: starting_balance,
        },
    );
    node.state.accounts.insert(
        from_b,
        fractal_core::Account {
            nonce: 0,
            balance: starting_balance,
        },
    );

    let raw_a = build_and_sign_1559(
        node.chain_id,
        0,
        1,
        10,
        21_000,
        Some(to_a),
        transfer_amount,
        vec![],
        &sk_a,
    );
    let hash_a = node
        .submit_raw_tx(&raw_a)
        .expect("accept first alternate MetaMask-style transfer");

    let raw_b = build_and_sign_1559(
        node.chain_id,
        0,
        1,
        10,
        21_000,
        Some(to_b),
        transfer_amount,
        vec![],
        &sk_b,
    );
    let hash_b = node
        .submit_raw_tx(&raw_b)
        .expect("accept second alternate MetaMask-style transfer");

    let node = Arc::new(Mutex::new(node));
    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    );

    let node = node.lock().await;
    assert_eq!(node.balance_of(&from_a), starting_balance - transfer_amount);
    assert_eq!(node.balance_of(&to_a), transfer_amount);
    assert_eq!(node.transaction_count(&from_a), 1);

    assert_eq!(node.balance_of(&from_b), starting_balance - transfer_amount);
    assert_eq!(node.balance_of(&to_b), transfer_amount);
    assert_eq!(node.transaction_count(&from_b), 1);

    assert!(node.mined_tx_info(&hash_a).is_some());
    assert!(node.mined_tx_info(&hash_b).is_some());
    assert_eq!(node.eth_signed_raw(&hash_a).as_deref(), Some(raw_a.as_slice()));
    assert_eq!(node.eth_signed_raw(&hash_b).as_deref(), Some(raw_b.as_slice()));
}

#[test]
fn node_accepts_eip1559_contract_create() {
    let mut node = NodeInner::devnet();

    let sk = SigningKey::from_bytes(&[9u8; 32].into()).unwrap();
    let from = addr_from_vk(sk.verifying_key());
    node.state.accounts.insert(from, fractal_core::Account { nonce: 0, balance: 10_000_000 });

    let init_code = vec![
        0x60, 0x01, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x01, 0x60, 0x00, 0xf3, 0x00,
    ];
    let raw = build_and_sign_1559(node.chain_id, 0, 1, 10, 1_000_000, None, 0, init_code.clone(), &sk);
    let tx_hash = node.submit_raw_tx(&raw).expect("accept raw eth create tx");
    let h = keccak256(&raw);
    assert_eq!(tx_hash, h);
    let stored = node.pending_txs.get(&h).expect("pending tx stored");
    assert_eq!(stored.signer, from);
    match &stored.body {
        TxBody::EvmCreate { init_code: got, .. } => assert_eq!(got, &init_code),
        _ => panic!("expected EvmCreate tx"),
    }
}

#[test]
fn eip1559_signed_json_has_type2_and_signature_components() {
    let mut node = NodeInner::devnet();
    let sk = SigningKey::from_bytes(&[11u8; 32].into()).unwrap();
    let from = addr_from_vk(sk.verifying_key());
    node.state.accounts.insert(from, fractal_core::Account { nonce: 0, balance: 10_000_000 });
    let to = [0x22u8; 20];
    let raw = build_and_sign_1559(node.chain_id, 0, 1, 10, 21_000, Some(to), 0, vec![], &sk);
    let h = node.submit_raw_tx(&raw).expect("accept raw eth tx");
    let got = node.eth_signed_raw(&h).expect("raw bytes stored");
    let j = eip1559_signed_tx_to_json(&got, None).expect("json");
    assert_eq!(j["type"], "0x2");
    assert!(j["r"].as_str().unwrap().starts_with("0x"));
    assert!(j["s"].as_str().unwrap().starts_with("0x"));
    assert!(j.get("yParity").is_some());
}

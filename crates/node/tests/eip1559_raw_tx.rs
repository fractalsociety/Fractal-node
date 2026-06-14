use k256::ecdsa::{
    signature::hazmat::PrehashSigner, RecoveryId, Signature, SigningKey, VerifyingKey,
};
use rlp::RlpStream;

use fractal_core::{TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_eth_wire::eip1559_signed_tx_to_json;
use fractal_node::NodeInner;
use fractal_rpc::ChainInteraction;

fn addr_from_vk(vk: &VerifyingKey) -> [u8; 20] {
    let uncompressed = vk.to_encoded_point(false);
    let pubkey = uncompressed.as_bytes();
    let h = keccak256(&pubkey[1..]);
    let mut a = [0u8; 20];
    a.copy_from_slice(&h[12..]);
    a
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
    node.state.accounts.insert(
        from,
        fractal_core::Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );

    let to = [0x11u8; 20];
    let raw = build_and_sign_1559(node.chain_id, 0, 1, 10, 21_000, Some(to), 123, vec![], &sk);
    node.submit_raw_tx(&raw).expect("accept raw eth tx");

    let h = keccak256(&raw);
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
fn node_accepts_eip1559_contract_create() {
    let mut node = NodeInner::devnet();

    let sk = SigningKey::from_bytes(&[9u8; 32].into()).unwrap();
    let from = addr_from_vk(sk.verifying_key());
    node.state.accounts.insert(
        from,
        fractal_core::Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );

    let init_code = vec![
        0x60, 0x01, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x01, 0x60, 0x00, 0xf3, 0x00,
    ];
    let raw = build_and_sign_1559(
        node.chain_id,
        0,
        1,
        10,
        1_000_000,
        None,
        0,
        init_code.clone(),
        &sk,
    );
    node.submit_raw_tx(&raw).expect("accept raw eth create tx");

    let h = keccak256(&raw);
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
    node.state.accounts.insert(
        from,
        fractal_core::Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );
    let to = [0x22u8; 20];
    let raw = build_and_sign_1559(node.chain_id, 0, 1, 10, 21_000, Some(to), 0, vec![], &sk);
    node.submit_raw_tx(&raw).expect("accept raw eth tx");
    let h = keccak256(&raw);
    let got = node.eth_signed_raw(&h).expect("raw bytes stored");
    let j = eip1559_signed_tx_to_json(&got, None).expect("json");
    assert_eq!(j["type"], "0x2");
    assert!(j["r"].as_str().unwrap().starts_with("0x"));
    assert!(j["s"].as_str().unwrap().starts_with("0x"));
    assert!(j.get("yParity").is_some());
}

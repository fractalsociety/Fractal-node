//! Determinism harness: fixed RNG + 10k txs → identical `state_root` across repeated runs.

use fractal_core::{apply_block, state_root, NativeCall, State, Transaction, TxBody, VmKind, Address};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

fn addr(i: u16) -> Address {
    let mut a = [0u8; 20];
    a[18..20].copy_from_slice(&i.to_be_bytes());
    a
}

fn fund_initial_state() -> State {
    let mut state = State::default();
    for i in 0..256u16 {
        state.accounts.insert(
            addr(i),
            fractal_core::Account {
                nonce: 0,
                balance: 10_000_000,
            },
        );
    }
    state
}

struct Sim {
    balances: [u128; 256],
    nonces: [u64; 256],
}

impl Sim {
    fn new() -> Self {
        Self {
            balances: [10_000_000; 256],
            nonces: [0; 256],
        }
    }

    fn build_txs(&mut self, rng: &mut StdRng) -> Vec<Transaction> {
        let mut txs = Vec::with_capacity(10_000);
        for _n in 0..10_000u32 {
            let pick = rng.gen_range(0..100u32);
            let signer_i = rng.gen_range(0..256usize);
            let signer = addr(signer_i as u16);

            if pick < 72 {
                // EVM transfer
                let mut recv_i = rng.gen_range(0..256usize);
                if recv_i == signer_i {
                    recv_i = (recv_i + 1) % 256;
                }
                let recv = addr(recv_i as u16);
                let max_amt = self.balances[signer_i].saturating_sub(1).min(500) as u32;
                if max_amt == 0 {
                    // fall back to native noop to keep stream length fixed at 10k
                    txs.push(Transaction {
                        signer,
                        nonce: self.nonces[signer_i],
                        vm: VmKind::Native,
                        body: TxBody::Native(NativeCall::NoOp),
                    });
                    self.nonces[signer_i] += 1;
                    continue;
                }
                let amount = 1 + rng.gen_range(0..max_amt) as u128;
                txs.push(Transaction {
                    signer,
                    nonce: self.nonces[signer_i],
                    vm: VmKind::Evm,
                    body: TxBody::Transfer { to: recv, amount },
                });
                self.balances[signer_i] -= amount;
                self.balances[recv_i] += amount;
                self.nonces[signer_i] += 1;
            } else {
                txs.push(Transaction {
                    signer,
                    nonce: self.nonces[signer_i],
                    vm: VmKind::Native,
                    body: TxBody::Native(NativeCall::NoOp),
                });
                self.nonces[signer_i] += 1;
            }
        }
        txs
    }
}

#[test]
fn ten_k_txs_state_root_is_identical_across_ten_runs() {
    let mut rng = StdRng::seed_from_u64(0xFAC1A1_C41A);
    let mut sim = Sim::new();
    let txs = sim.build_txs(&mut rng);
    assert_eq!(txs.len(), 10_000);

    let mut roots = Vec::new();
    for _ in 0..10 {
        let mut st = fund_initial_state();
        apply_block(&mut st, &txs).expect("all generated txs must be valid");
        roots.push(state_root(&st).expect("commit"));
    }
    assert!(roots.windows(2).all(|w| w[0] == w[1]));
}

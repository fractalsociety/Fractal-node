//! 20-byte address (shared EVM / native account space).

use fractal_crypto::hash::keccak256;
use rlp::RlpStream;

pub type Address = [u8; 20];

/// Ethereum `CREATE` address: `keccak256(rlp([deployer, nonce]))[12..]`.
pub fn create_contract_address(deployer: Address, nonce: u64) -> Address {
    let mut s = RlpStream::new_list(2);
    s.append(&deployer.as_slice());
    s.append(&nonce);
    let h = keccak256(&s.out());
    let mut a = [0u8; 20];
    a.copy_from_slice(&h[12..]);
    a
}

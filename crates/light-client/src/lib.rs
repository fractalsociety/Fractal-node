//! Light-client verifier: check Plonky2 SNARK + `globalStateRoot` without a full node.
//!
//! PRD §7.10: trust masterchain finality → verify `globalZkRoot` → read shard anchors.
//!
//! ```ignore
//! use fractal_light_client::fetch_and_verify_light_client_head;
//! let verified = fetch_and_verify_light_client_head("http://127.0.0.1:8545")?;
//! let root = verified.shard_state_root(0).expect("shard 0 anchor");
//! ```

mod error;
mod head;
mod parse;
mod rpc;
mod verify;

pub use error::LightClientError;
pub use head::{LightClientHeadV1, VerifiedLightClientHead};
pub use parse::parse_light_client_head_json;
pub use rpc::{fetch_and_verify_light_client_head, fetch_light_client_head};
pub use verify::{verify_light_client_head, verify_masterchain_block};

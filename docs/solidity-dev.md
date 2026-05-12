# Solidity on FractalChain (devnet)

This page supports **PRD §18 M4** exit criteria: deploy a contract with Hardhat (or another stack), call Fractal native precompiles from Solidity, and use standard JSON-RPC wallets.

## Toolchain

- **Solidity**: `^0.8.20` (see `contracts/examples/`).
- **RPC**: Ethereum-compatible JSON-RPC on the dev node (chain id **41** in the default `NodeInner::devnet()`).
- **Calldata encoding**: Native precompiles expect **borsh** payloads matching Rust `NativeCall` in `crates/core/src/tx.rs`. When in doubt, build bytes in Rust or TypeScript (borsh) and pass them into your contract.

## Fractal native precompile addresses (PRD §9.3)

- Byte **0** of the 20-byte address is always **`0xfc`**.
- Byte **1** is a reserved **opcode slot** in **`0x01` … `0x0d`** (inclusive). Remaining bytes are zero today.
- Example probe address: **`0xFC01000000000000000000000000000000000000`** (`opcode = 0x01`).

The execution layer decodes **calldata** as borsh `NativeCall`; the second address byte is reserved for routing / future checks. Example tests use slot `0x01` with a `NoOp` payload.

## Stable `NativeCall::NoOp` wire format

For smoke tests and minimal Solidity demos, `NativeCall::NoOp` serializes to the single byte **`0x0d`**. This is asserted in `crates/core/tests/native_call_borsh_snapshots.rs` so docs and contracts stay aligned with the enum layout.

## Example contracts

| File | Role |
|------|------|
| `contracts/examples/FractalNative.sol` | `syscallAddress` / `syscallCall` helpers. |
| `contracts/examples/AgentBountyEscrow.sol` | PRD **AgentBountyEscrow** pattern: bounty `mapping` + `pingNativeNoOp` / `forwardNative`. |

Compile with your usual pipeline, for example:

```bash
solc --bin --optimize contracts/examples/FractalNative.sol contracts/examples/AgentBountyEscrow.sol
```

or a Foundry project that lists `contracts/examples` as a source root.

## In-repo Hardhat (`contracts/`)

The repo includes a minimal **Hardhat** package under `contracts/` (`hardhat.config.cjs`, `scripts/deploy.js`). With **`fractal-node`** listening on JSON-RPC (default `http://127.0.0.1:8545`, overridable via `FRACTAL_RPC_URL`):

```bash
./scripts/deploy-fractal-contracts.sh
```

That script installs npm dependencies, compiles, and runs `npm run deploy` against the configured network (`fractalLocal`, chain id **41**).

## JSON-RPC and transactions

- Use **`eth_sendRawTransaction`** with **EIP-1559 (type `0x02`)** bytes for `VmKind::Evm` transactions (see `crates/node/src/eth_signed.rs` and `crates/node/tests/eip1559_raw_tx.rs`).
- **`EvmCall.value`** must be **zero** on the current devnet state machine (`State::apply_transaction_with_evm`); do not rely on payable EVM calls carrying value until that restriction is lifted.

## MetaMask

Add a custom network with the dev node RPC URL and **chain id 41**. Fund the deployer account in devnet genesis / faucet flows as implemented for your environment.

## Related PRD sections

- **§9** — Native VM + EVM shared state, precompile namespace.
- **§14** — JSON-RPC surface (MetaMask / ethers.js).
- **§18 M4** — EVM integration deliverables and exit criteria.

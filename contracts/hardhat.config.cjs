require("@nomicfoundation/hardhat-ethers");

/** Same private key as Hardhat default #0 — must match `fractal_node::HARDHAT_DEFAULT_SIGNER_0` prefund in devnet. */
const HARDHAT_DEFAULT_KEY =
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

/** @type import("hardhat/config").HardhatUserConfig */
module.exports = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: { enabled: true, runs: 200 },
    },
  },
  paths: {
    sources: "./examples",
    cache: "./cache",
    artifacts: "./artifacts",
  },
  networks: {
    fractalLocal: {
      url: process.env.FRACTAL_RPC_URL || "http://127.0.0.1:8545",
      chainId: 41,
      accounts: [process.env.FRACTAL_PRIVATE_KEY || HARDHAT_DEFAULT_KEY],
    },
  },
};

/**
 * Deploy `AgentBountyEscrow` to a running fractal-node (`run_dev`).
 *
 * Usage: `npm run deploy` (see repo `scripts/deploy-fractal-contracts.sh`).
 */
const { ethers } = require("hardhat");

async function main() {
  const [deployer] = await ethers.getSigners();
  const net = await ethers.provider.getNetwork();
  const balance = await ethers.provider.getBalance(deployer.address);
  console.log(
    JSON.stringify(
      {
        step: "deploy_start",
        deployer: deployer.address,
        chainId: net.chainId.toString(),
        balanceWei: balance.toString(),
        rpcUrl: (process.env.FRACTAL_RPC_URL || "http://127.0.0.1:8545").replace(/\/$/, ""),
      },
      null,
      2,
    ),
  );

  const Factory = await ethers.getContractFactory("AgentBountyEscrow");
  const feeOverrides = {
    type: 2,
    maxFeePerGas: 100_000_000_000n,
    maxPriorityFeePerGas: 2_000_000_000n,
  };
  const contract = await Factory.deploy(feeOverrides);
  await contract.waitForDeployment();
  const address = await contract.getAddress();

  console.log(
    JSON.stringify(
      {
        step: "deploy_done",
        contract: "AgentBountyEscrow",
        address,
        chainId: net.chainId.toString(),
      },
      null,
      2,
    ),
  );
}

main().catch((err) => {
  console.error(
    JSON.stringify(
      {
        step: "deploy_failed",
        message: err.shortMessage || err.message || String(err),
        details: err.info || undefined,
      },
      null,
      2,
    ),
  );
  process.exit(1);
});

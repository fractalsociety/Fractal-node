/**
 * Deploy `AgentBountyEscrow`, then `openBounty` + `pingNativeNoOp` (native precompile smoke).
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

  // PRD M4 exit criteria: call Fractal native precompile from deployed Solidity (`pingNativeNoOp` → `NativeCall::NoOp`).
  const bountyId = ethers.id("prd-m4-example-bounty");
  const openTx = await contract.openBounty(bountyId, feeOverrides);
  const openRc = await openTx.wait();
  console.log(
    JSON.stringify(
      {
        step: "open_bounty_mined",
        txHash: openRc.hash,
        bountyId,
      },
      null,
      2,
    ),
  );

  const pingTx = await contract.pingNativeNoOp(feeOverrides);
  const pingRc = await pingTx.wait();
  let nativeOk = false;
  for (const log of pingRc.logs) {
    try {
      const parsed = contract.interface.parseLog({ topics: log.topics, data: log.data });
      if (parsed?.name === "NativeCallResult") {
        nativeOk = Boolean(parsed.args.success);
        break;
      }
    } catch {
      // not this contract / event
    }
  }
  console.log(
    JSON.stringify(
      {
        step: "ping_native_noop_mined",
        txHash: pingRc.hash,
        nativeCallSuccess: nativeOk,
        status: pingRc.status,
      },
      null,
      2,
    ),
  );
  if (!nativeOk) {
    throw new Error(
      JSON.stringify({
        step: "ping_native_failed",
        message: "expected NativeCallResult(true) from pingNativeNoOp",
        receiptLogs: pingRc.logs.length,
      }),
    );
  }
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

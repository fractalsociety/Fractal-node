use fractal_bench::run_protocol_bench;

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let certificates = parse_usize_arg(&args, "--certificates")?.unwrap_or(1_000);
    let validators = parse_usize_arg(&args, "--validators")?.unwrap_or(7);
    let quorum = parse_usize_arg(&args, "--quorum")?.unwrap_or(5);
    let da_payload_bytes = parse_usize_arg(&args, "--da-payload-bytes")?.unwrap_or(4 * 1024 * 1024);
    let da_share_size = parse_u32_arg(&args, "--da-share-size")?.unwrap_or(32 * 1024);
    let da_samples = parse_usize_arg(&args, "--da-samples")?.unwrap_or(256);
    let da_rounds = parse_usize_arg(&args, "--da-rounds")?.unwrap_or(1_000);
    let proofs = parse_usize_arg(&args, "--proofs")?.unwrap_or(1_000);
    let proof_covered_blocks = parse_u64_arg(&args, "--proof-covered-blocks")?.unwrap_or(1);
    let prover_cost_micro_frac_per_block =
        parse_i128_arg(&args, "--prover-cost-micro-frac-per-block")?.unwrap_or(0);
    let seed = parse_u64_arg(&args, "--seed")?.unwrap_or(41);

    let report = run_protocol_bench(
        certificates,
        validators,
        quorum,
        da_payload_bytes,
        da_share_size,
        da_samples,
        da_rounds,
        proofs,
        proof_covered_blocks,
        prover_cost_micro_frac_per_block,
        seed,
    );
    let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json encode: {e}"))?;
    println!("{json}");
    Ok(())
}

fn parse_string_arg(args: &[String], name: &str) -> Result<Option<String>, String> {
    let mut i = 1;
    while i < args.len() {
        if args[i] == name {
            return args
                .get(i + 1)
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("{name} requires a value"));
        }
        i += 1;
    }
    Ok(None)
}

fn parse_usize_arg(args: &[String], name: &str) -> Result<Option<usize>, String> {
    parse_string_arg(args, name)?
        .map(|v| v.parse::<usize>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

fn parse_u32_arg(args: &[String], name: &str) -> Result<Option<u32>, String> {
    parse_string_arg(args, name)?
        .map(|v| v.parse::<u32>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

fn parse_u64_arg(args: &[String], name: &str) -> Result<Option<u64>, String> {
    parse_string_arg(args, name)?
        .map(|v| v.parse::<u64>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

fn parse_i128_arg(args: &[String], name: &str) -> Result<Option<i128>, String> {
    parse_string_arg(args, name)?
        .map(|v| v.parse::<i128>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

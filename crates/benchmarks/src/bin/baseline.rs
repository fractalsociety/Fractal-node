use fractal_bench::{run_baseline_bench, BaselineBenchConfig};

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let config = BaselineBenchConfig {
        blocks_per_scenario: parse_usize_arg(&args, "--blocks")?.unwrap_or(16),
        txs_per_block: parse_usize_arg(&args, "--txs-per-block")?.unwrap_or(64),
        chain_id: parse_u64_arg(&args, "--chain-id")?.unwrap_or(41),
        gas_limit: parse_u64_arg(&args, "--gas-limit")?.unwrap_or(60_000_000),
        seed: parse_u64_arg(&args, "--seed")?.unwrap_or(41),
    };
    let output = parse_string_arg(&args, "--output")?;
    let report = run_baseline_bench(config);
    let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json encode: {e}"))?;
    if let Some(path) = output {
        std::fs::write(&path, format!("{json}\n")).map_err(|e| format!("{path}: {e}"))?;
    } else {
        println!("{json}");
    }
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

fn parse_u64_arg(args: &[String], name: &str) -> Result<Option<u64>, String> {
    parse_string_arg(args, name)?
        .map(|v| v.parse::<u64>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

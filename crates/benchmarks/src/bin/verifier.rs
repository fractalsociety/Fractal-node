use fractal_bench::{
    generate_verifier_cases, run_verifier_bench, synthetic_verifier_judgments, VerifierJudgment,
};
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let judgments_path = parse_string_arg(&args, "--judgments")?;
    let cases = generate_verifier_cases();
    let judgments = if let Some(path) = judgments_path {
        read_judgments_jsonl(&path)?
    } else {
        synthetic_verifier_judgments(&cases)
    };
    let report = run_verifier_bench(&cases, &judgments);
    let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json encode: {e}"))?;
    println!("{json}");
    Ok(())
}

fn read_judgments_jsonl(path: &str) -> Result<Vec<VerifierJudgment>, String> {
    let file = File::open(path).map_err(|e| format!("open judgments: {e}"))?;
    let mut judgments = Vec::new();
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("read judgments line {}: {e}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let judgment = serde_json::from_str::<VerifierJudgment>(&line)
            .map_err(|e| format!("parse judgments line {}: {e}", i + 1))?;
        judgments.push(judgment);
    }
    Ok(judgments)
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

use fractal_bench::{
    generate_economic_tasks, run_economic_bench, synthetic_attempts, ModelAttempt,
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
    let task_count = parse_usize_arg(&args, "--tasks")?.unwrap_or(90);
    let seed = parse_u64_arg(&args, "--seed")?.unwrap_or(41);
    let attempts_path = parse_string_arg(&args, "--attempts")?;
    let tasks = generate_economic_tasks(task_count, seed);
    let attempts = if let Some(path) = attempts_path {
        read_attempts_jsonl(&path)?
    } else {
        synthetic_attempts(&tasks)
    };
    let report = run_economic_bench(&tasks, &attempts);
    let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json encode: {e}"))?;
    println!("{json}");
    Ok(())
}

fn read_attempts_jsonl(path: &str) -> Result<Vec<ModelAttempt>, String> {
    let file = File::open(path).map_err(|e| format!("open attempts: {e}"))?;
    let mut attempts = Vec::new();
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| format!("read attempts line {}: {e}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let attempt = serde_json::from_str::<ModelAttempt>(&line)
            .map_err(|e| format!("parse attempts line {}: {e}", i + 1))?;
        attempts.push(attempt);
    }
    Ok(attempts)
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

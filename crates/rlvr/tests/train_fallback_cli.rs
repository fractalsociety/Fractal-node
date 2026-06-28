//! RLVR-032 end-to-end: `fractal-rlvr train --mode dpo|sft` dispatches through
//! `run_argv` to the fallback DPO/SFT CLI, reads scored rollouts, builds the
//! dataset, and writes it to `--out`.

use fractal_rlvr::run_argv;

fn rollout(task_id: &str, prompt: &str, response: &str, reward: f64) -> serde_json::Value {
    serde_json::json!({
        "task_id": task_id,
        "prompt": prompt,
        "response": response,
        "reward": reward,
        "trace_id": format!("trace-{task_id}"),
    })
}

fn write_rollouts(path: &std::path::Path, rows: &[serde_json::Value]) {
    let mut content = String::new();
    for row in rows {
        content.push_str(&serde_json::to_string(row).unwrap());
        content.push('\n');
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn train_mode_dpo_builds_and_writes_preference_pairs() {
    let dir =
        std::env::temp_dir().join(format!("fractal-rlvr-train-cli-dpo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let rollouts_path = dir.join("rollouts.jsonl");
    write_rollouts(
        &rollouts_path,
        &[
            rollout("good", "What is 2+2?", "4", 0.95),
            rollout("bad", "What is 2+2?", "5?", 0.10),
        ],
    );

    let out_dir = dir.join("out");
    let argv = vec![
        "fractal-rlvr".to_string(),
        "train".into(),
        "--mode".into(),
        "dpo".into(),
        "--rollouts".into(),
        rollouts_path.display().to_string(),
        "--out".into(),
        out_dir.display().to_string(),
    ];
    let summary = run_argv(&argv).expect("train --mode dpo dispatches and succeeds");
    assert!(summary.contains("train --mode dpo"));
    assert!(summary.contains("produced=1"));
    assert!(out_dir.join("dpo_pairs.jsonl").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn train_mode_sft_filters_to_high_reward_examples() {
    let dir =
        std::env::temp_dir().join(format!("fractal-rlvr-train-cli-sft-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let rollouts_path = dir.join("rollouts.jsonl");
    write_rollouts(
        &rollouts_path,
        &[
            rollout("good", "p1", "r1", 0.95),
            rollout("weak", "p2", "r2", 0.30),
        ],
    );

    let out_dir = dir.join("out");
    let argv = vec![
        "fractal-rlvr".to_string(),
        "train".into(),
        "--mode".into(),
        "sft".into(),
        "--rollouts".into(),
        rollouts_path.display().to_string(),
        "--out".into(),
        out_dir.display().to_string(),
    ];
    let summary = run_argv(&argv).expect("train --mode sft dispatches and succeeds");
    assert!(summary.contains("train --mode sft"));
    assert!(summary.contains("produced=1")); // only the 0.95 rollout passes the 0.70 threshold
    assert!(out_dir.join("sft_examples.jsonl").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn train_without_fallback_mode_stays_registered_for_grpo() {
    // No --mode dpo|sft -> GRPO path is not wired yet, so `train` stays registered.
    let out = run_argv(&["fractal-rlvr".into(), "train".into()]).unwrap();
    assert!(out.contains("registered for later implementation"));
}

#[test]
fn train_mode_dpo_rejects_missing_rollouts_file_args() {
    let argv = vec![
        "fractal-rlvr".to_string(),
        "train".into(),
        "--mode".into(),
        "dpo".into(),
    ];
    assert!(run_argv(&argv).is_err());
}

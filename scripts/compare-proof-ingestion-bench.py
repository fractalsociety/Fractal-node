#!/usr/bin/env python3
"""Compare baseline and proof-ingestion benchmark JSON summaries.

The H3 report intentionally accepts schema-compatible H1/H2 reports, older
flat load-TPS summaries, and the current protocol micro-benchmark report. It
normalizes each input into named scenarios with numeric metrics, computes
per-metric deltas, classifies the likely bottleneck, and emits JSON plus
human-readable Markdown/HTML.
"""

from __future__ import annotations

import argparse
import html
import json
import math
from pathlib import Path
from typing import Any


BOTTLENECK_KEYWORDS = {
    "consensus": (
        "bft",
        "block",
        "latency",
        "qc",
        "quorum",
        "vote",
        "validator",
    ),
    "proof verification": (
        "proof",
        "verify",
        "verified",
        "verification",
        "witness",
        "aggregation",
        "circuit",
    ),
    "DA sampling": (
        "da",
        "sample",
        "share",
        "encoded",
        "namespace",
    ),
    "network": (
        "submit",
        "confirmed",
        "tps",
        "throughput",
        "payload",
        "errors",
        "certificate",
        "cert",
    ),
    "storage": (
        "bytes",
        "replay",
        "working",
        "memory",
        "store",
        "state",
    ),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, help="H1 baseline JSON summary")
    parser.add_argument("--proof", required=True, help="H2 proof-ingestion JSON summary")
    parser.add_argument("--json", dest="json_out", help="comparison JSON output path")
    parser.add_argument("--markdown", "--md", dest="markdown_out", help="markdown output path")
    parser.add_argument("--html", dest="html_out", help="HTML output path")
    parser.add_argument(
        "--title",
        default="Proof-Ingestion Benchmark Comparison",
        help="report title",
    )
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    proof_path = Path(args.proof)
    baseline_raw = read_json(baseline_path)
    proof_raw = read_json(proof_path)

    report = build_report(args.title, baseline_path, proof_path, baseline_raw, proof_raw)

    if args.json_out:
        write_text(Path(args.json_out), json.dumps(report, indent=2, sort_keys=True) + "\n")
    if args.markdown_out:
        write_text(Path(args.markdown_out), render_markdown(report))
    if args.html_out:
        write_text(Path(args.html_out), render_html(report))
    if not args.json_out and not args.markdown_out and not args.html_out:
        print(render_markdown(report), end="")
    return 0


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError as exc:
        raise SystemExit(f"missing JSON file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from exc


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def build_report(
    title: str,
    baseline_path: Path,
    proof_path: Path,
    baseline_raw: Any,
    proof_raw: Any,
) -> dict[str, Any]:
    baseline = normalize_summary(baseline_raw)
    proof = normalize_summary(proof_raw)
    scenario_names = sorted(set(baseline) | set(proof))
    scenarios = []
    for name in scenario_names:
        b_metrics = baseline.get(name, {})
        p_metrics = proof.get(name, {})
        comparisons = compare_metrics(b_metrics, p_metrics)
        scenarios.append(
            {
                "scenario": name,
                "bottleneck": classify_bottleneck(name, comparisons),
                "baselineOnly": name not in proof,
                "proofOnly": name not in baseline,
                "metrics": comparisons,
            }
        )

    return {
        "schemaVersion": 1,
        "title": title,
        "baselinePath": str(baseline_path),
        "proofPath": str(proof_path),
        "baselineRunKind": string_or_none(baseline_raw, "runKind", "run_kind", "label"),
        "proofRunKind": string_or_none(proof_raw, "runKind", "run_kind", "label"),
        "scenarioCount": len(scenarios),
        "scenarios": scenarios,
    }


def normalize_summary(raw: Any) -> dict[str, dict[str, float]]:
    if not isinstance(raw, dict):
        raise SystemExit("benchmark summary root must be a JSON object")

    if isinstance(raw.get("scenarios"), list):
        out: dict[str, dict[str, float]] = {}
        for idx, scenario in enumerate(raw["scenarios"]):
            if not isinstance(scenario, dict):
                continue
            name = str(scenario.get("name") or scenario.get("kind") or f"scenario-{idx}")
            out[canonical_scenario_name(name)] = flatten_numeric(scenario)
        return out

    protocol_sections = {
        "ownedObjectCertificates": "certificate-updates",
        "owned_object_certificates": "certificate-updates",
        "daSampling": "da-sampling",
        "da_sampling": "da-sampling",
        "proofLatencyCost": "proof-updates",
        "proof_latency_cost": "proof-updates",
        "mixedProofSlo": "mixed-shared-state",
        "mixed_proof_slo": "mixed-shared-state",
    }
    protocol_out = {}
    for key, name in protocol_sections.items():
        section = raw.get(key)
        if isinstance(section, dict):
            protocol_out[name] = flatten_numeric(section)
    if protocol_out:
        return protocol_out

    return {canonical_scenario_name("load-tps"): flatten_numeric(raw)}


def canonical_scenario_name(name: str) -> str:
    normalized = camel_to_words(name).replace("_", "-")
    aliases = {
        "proof-commitment": "proof-updates",
        "proof-updates": "proof-updates",
        "accepted-proof-updates": "proof-updates",
        "da-sampling-proof-updates": "da-sampling",
        "owned-object-tx": "certificate-updates",
        "certificate-updates": "certificate-updates",
        "accepted-certificate-updates": "certificate-updates",
        "mixed-evm-native": "mixed-shared-state",
        "mixed-proof-shared-state": "mixed-shared-state",
        "mixed-proof-updates-shared-state": "mixed-shared-state",
        "bft-7-validator-lab": "bft-7",
        "bft-7-proof-ingestion": "bft-7",
    }
    return aliases.get(normalized, normalized)


def flatten_numeric(value: Any, prefix: str = "") -> dict[str, float]:
    out: dict[str, float] = {}
    if isinstance(value, dict):
        for key, child in value.items():
            child_key = camel_to_words(str(key))
            path = f"{prefix}.{child_key}" if prefix else child_key
            out.update(flatten_numeric(child, path))
    elif isinstance(value, list):
        return out
    elif isinstance(value, bool):
        return out
    elif isinstance(value, (int, float)) and math.isfinite(float(value)):
        out[prefix] = float(value)
    return out


def compare_metrics(
    baseline: dict[str, float],
    proof: dict[str, float],
) -> list[dict[str, Any]]:
    metrics = []
    for key in sorted(set(baseline) | set(proof)):
        b = baseline.get(key)
        p = proof.get(key)
        delta = None if b is None or p is None else p - b
        pct = None
        if b is not None and p is not None and b != 0:
            pct = (p - b) / abs(b) * 100.0
        metrics.append(
            {
                "metric": key,
                "baseline": b,
                "proof": p,
                "delta": delta,
                "deltaPct": pct,
                "direction": direction_for_metric(key),
            }
        )
    return metrics


def classify_bottleneck(scenario: str, metrics: list[dict[str, Any]]) -> str:
    preferred = {
        "bft-7": "consensus",
        "certificate-updates": "network",
        "da-sampling": "DA sampling",
        "proof-updates": "proof verification",
    }
    if scenario in preferred:
        return preferred[scenario]

    scores = {category: 0.0 for category in BOTTLENECK_KEYWORDS}
    words = scenario.lower().replace("-", " ").replace("_", " ")
    for category, keywords in BOTTLENECK_KEYWORDS.items():
        if any(keyword in words for keyword in keywords):
            scores[category] += 25.0
    if "bft" in words:
        scores["consensus"] += 25.0
    if "proof" in words:
        scores["proof verification"] += 25.0
    if "da" in words or "sampling" in words:
        scores["DA sampling"] += 25.0
    if "certificate" in words or "cert" in words:
        scores["network"] += 25.0

    for metric in metrics:
        key = str(metric["metric"]).lower()
        value = metric.get("proof")
        baseline = metric.get("baseline")
        weight = abs(float(value or 0.0))
        if metric.get("delta") is not None:
            weight = max(weight, abs(float(metric["delta"])))
        if baseline is not None and value is not None and baseline != 0:
            weight += abs((value - baseline) / baseline) * 10.0
        weight = math.log10(weight + 10.0)
        for category, keywords in BOTTLENECK_KEYWORDS.items():
            if any(keyword in key for keyword in keywords):
                scores[category] += weight

    return max(scores.items(), key=lambda item: item[1])[0]


def direction_for_metric(metric: str) -> str:
    lower = metric.lower()
    if any(token in lower for token in ("tps", "per second", "per_second", "rate", "throughput", "verified", "accepted", "committed")):
        return "higher_is_better"
    if any(token in lower for token in ("latency", "nanos", "micros", "bytes", "errors", "cost", "fee", "memory", "working", "replay")):
        return "lower_is_better"
    return "informational"


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        f"# {report['title']}",
        "",
        f"- baseline JSON: `{report['baselinePath']}`",
        f"- proof-ingestion JSON: `{report['proofPath']}`",
        f"- scenarios compared: {report['scenarioCount']}",
        "",
        "## Scenario Summary",
        "",
        "| Scenario | Bottleneck | Baseline-only | Proof-only | Metrics |",
        "|---|---|---:|---:|---:|",
    ]
    for scenario in report["scenarios"]:
        lines.append(
            "| {scenario} | {bottleneck} | {baseline_only} | {proof_only} | {count} |".format(
                scenario=scenario["scenario"],
                bottleneck=scenario["bottleneck"],
                baseline_only="yes" if scenario["baselineOnly"] else "no",
                proof_only="yes" if scenario["proofOnly"] else "no",
                count=len(scenario["metrics"]),
            )
        )

    for scenario in report["scenarios"]:
        lines.extend(
            [
                "",
                f"## {scenario['scenario']}",
                "",
                f"Dominant bottleneck: **{scenario['bottleneck']}**",
                "",
                "| Metric | Baseline | Proof ingestion | Delta | Delta % | Direction |",
                "|---|---:|---:|---:|---:|---|",
            ]
        )
        for metric in scenario["metrics"]:
            lines.append(
                "| {metric} | {baseline} | {proof} | {delta} | {pct} | {direction} |".format(
                    metric=metric["metric"],
                    baseline=format_value(metric["baseline"]),
                    proof=format_value(metric["proof"]),
                    delta=format_value(metric["delta"], signed=True),
                    pct=format_percent(metric["deltaPct"]),
                    direction=metric["direction"],
                )
            )
    lines.append("")
    return "\n".join(lines)


def render_html(report: dict[str, Any]) -> str:
    rows = []
    for scenario in report["scenarios"]:
        rows.append(
            "<tr>"
            f"<td>{html.escape(scenario['scenario'])}</td>"
            f"<td>{html.escape(scenario['bottleneck'])}</td>"
            f"<td>{'yes' if scenario['baselineOnly'] else 'no'}</td>"
            f"<td>{'yes' if scenario['proofOnly'] else 'no'}</td>"
            f"<td>{len(scenario['metrics'])}</td>"
            "</tr>"
        )
    sections = [
        "<h2>Scenario Summary</h2>",
        "<table><thead><tr><th>Scenario</th><th>Bottleneck</th><th>Baseline-only</th><th>Proof-only</th><th>Metrics</th></tr></thead>"
        f"<tbody>{''.join(rows)}</tbody></table>",
    ]
    for scenario in report["scenarios"]:
        metric_rows = []
        for metric in scenario["metrics"]:
            metric_rows.append(
                "<tr>"
                f"<td>{html.escape(metric['metric'])}</td>"
                f"<td>{format_value(metric['baseline'])}</td>"
                f"<td>{format_value(metric['proof'])}</td>"
                f"<td>{format_value(metric['delta'], signed=True)}</td>"
                f"<td>{format_percent(metric['deltaPct'])}</td>"
                f"<td>{html.escape(metric['direction'])}</td>"
                "</tr>"
            )
        sections.append(
            f"<h2>{html.escape(scenario['scenario'])}</h2>"
            f"<p>Dominant bottleneck: <strong>{html.escape(scenario['bottleneck'])}</strong></p>"
            "<table><thead><tr><th>Metric</th><th>Baseline</th><th>Proof ingestion</th><th>Delta</th><th>Delta %</th><th>Direction</th></tr></thead>"
            f"<tbody>{''.join(metric_rows)}</tbody></table>"
        )
    body = (
        f"<h1>{html.escape(report['title'])}</h1>"
        f"<p>Baseline JSON: <code>{html.escape(report['baselinePath'])}</code><br>"
        f"Proof-ingestion JSON: <code>{html.escape(report['proofPath'])}</code><br>"
        f"Scenarios compared: {report['scenarioCount']}</p>"
        + "".join(sections)
    )
    return (
        "<!doctype html>\n"
        "<html><head><meta charset=\"utf-8\">"
        f"<title>{html.escape(report['title'])}</title>"
        "<style>body{font:14px system-ui;margin:32px;line-height:1.4;}"
        "table{border-collapse:collapse;margin:16px 0 28px;width:100%;}"
        "th,td{border:1px solid #d0d7de;padding:6px 8px;text-align:right;}"
        "th:first-child,td:first-child,th:nth-child(2),td:nth-child(2),th:last-child,td:last-child{text-align:left;}"
        "th{background:#f6f8fa;}code{background:#f6f8fa;padding:2px 4px;}</style>"
        "</head><body>"
        f"{body}"
        "</body></html>\n"
    )


def format_value(value: Any, signed: bool = False) -> str:
    if value is None:
        return "n/a"
    number = float(value)
    if abs(number) >= 1000 or number == int(number):
        text = f"{number:,.0f}"
    else:
        text = f"{number:.4f}"
    if signed and number > 0:
        return f"+{text}"
    return text


def format_percent(value: Any) -> str:
    if value is None:
        return "n/a"
    number = float(value)
    sign = "+" if number > 0 else ""
    return f"{sign}{number:.2f}%"


def camel_to_words(value: str) -> str:
    out = []
    prev_lower = False
    for ch in value:
        if ch == "_":
            out.append(" ")
            prev_lower = False
        elif ch.isupper() and prev_lower:
            out.append(" ")
            out.append(ch.lower())
            prev_lower = False
        else:
            out.append(ch.lower())
            prev_lower = ch.islower() or ch.isdigit()
    return "".join(out).strip().replace(" ", "_")


def string_or_none(raw: Any, *keys: str) -> str | None:
    if not isinstance(raw, dict):
        return None
    for key in keys:
        value = raw.get(key)
        if isinstance(value, str):
            return value
    return None


if __name__ == "__main__":
    raise SystemExit(main())

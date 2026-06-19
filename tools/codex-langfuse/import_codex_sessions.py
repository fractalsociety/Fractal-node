#!/usr/bin/env python3
"""Normalize local Codex sessions and optionally import them into Langfuse."""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Iterable


DEFAULT_CODEX_HOME = Path.home() / ".codex"
DEFAULT_OUTPUT = Path(__file__).resolve().parent / "export" / "codex-sessions.normalized.jsonl"

SECRET_PATTERNS = [
    (re.compile(r"ghp_[A-Za-z0-9_]{20,}"), "ghp_[REDACTED]"),
    (re.compile(r"github_pat_[A-Za-z0-9_]{20,}"), "github_pat_[REDACTED]"),
    (re.compile(r"sk-[A-Za-z0-9_-]{20,}"), "sk-[REDACTED]"),
    (re.compile(r"pk-[A-Za-z0-9_-]{20,}"), "pk-[REDACTED]"),
    (re.compile(r"(?i)(bearer\s+)[A-Za-z0-9._~+/=-]{20,}"), r"\1[REDACTED]"),
    (re.compile(r"(?i)((?:api[_-]?key|token|secret|password)\s*[:=]\s*)[^\s\"']+"), r"\1[REDACTED]"),
]


@dataclasses.dataclass
class NormalizedEvent:
    timestamp: str
    kind: str
    role: str | None
    text: str
    metadata: dict[str, Any]


@dataclasses.dataclass
class NormalizedSession:
    session_id: str
    source_file: str
    started_at: str | None
    cwd: str | None
    originator: str | None
    cli_version: str | None
    model_provider: str | None
    events: list[NormalizedEvent]

    def digest(self) -> str:
        encoded = canonical_json(dataclasses.asdict(self)).encode("utf-8")
        return hashlib.sha256(encoded).hexdigest()


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def redact_text(text: str) -> str:
    redacted = text
    for pattern, replacement in SECRET_PATTERNS:
        redacted = pattern.sub(replacement, redacted)
    return redacted


def truncate_text(text: str, max_chars: int) -> str:
    if max_chars <= 0 or len(text) <= max_chars:
        return text
    omitted = len(text) - max_chars
    return f"{text[:max_chars]}\n...[truncated {omitted} chars]"


def parse_ts(value: str | None) -> dt.datetime | None:
    if not value:
        return None
    try:
        return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def in_since_window(timestamp: str | None, since_days: int | None) -> bool:
    if since_days is None:
        return True
    parsed = parse_ts(timestamp)
    if parsed is None:
        return True
    cutoff = dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=since_days)
    return parsed >= cutoff


def text_from_content(content: Any) -> str:
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""

    parts: list[str] = []
    for item in content:
        if not isinstance(item, dict):
            continue
        for key in ("text", "input_text", "output_text"):
            value = item.get(key)
            if isinstance(value, str):
                parts.append(value)
                break
    return "\n".join(parts)


def extract_event(record: dict[str, Any], max_chars: int) -> NormalizedEvent | None:
    timestamp = str(record.get("timestamp") or "")
    record_type = str(record.get("type") or "unknown")
    payload = record.get("payload")
    if not isinstance(payload, dict):
        return None

    metadata: dict[str, Any] = {"record_type": record_type}
    role: str | None = None
    text = ""
    kind = record_type

    if record_type == "response_item":
        payload_type = str(payload.get("type") or "response_item")
        kind = payload_type
        role = payload.get("role") if isinstance(payload.get("role"), str) else None
        text = text_from_content(payload.get("content"))
        metadata["payload_type"] = payload_type
        if payload_type == "function_call":
            kind = "tool_call"
            tool_name = str(payload.get("name") or "tool")
            arguments = payload.get("arguments")
            text = f"{tool_name} {arguments}" if arguments is not None else tool_name
            metadata["tool_name"] = tool_name
        elif payload_type == "function_call_output":
            kind = "tool_output"
            text = str(payload.get("output") or "")
    elif record_type == "event_msg":
        event_type = str(payload.get("type") or "event")
        kind = event_type
        if "message" in payload:
            text = str(payload.get("message") or "")
        else:
            text = canonical_json(payload)
        metadata["event_type"] = event_type
    elif record_type == "turn_context":
        kind = "turn_context"
        text = ""
        metadata.update({
            "cwd": payload.get("cwd"),
            "model": payload.get("model"),
            "approval_policy": payload.get("approval_policy"),
        })
    else:
        text = canonical_json(payload)

    text = truncate_text(redact_text(text), max_chars)
    return NormalizedEvent(
        timestamp=timestamp,
        kind=kind,
        role=role,
        text=text,
        metadata={k: v for k, v in metadata.items() if v is not None},
    )


def session_files(codex_home: Path) -> list[Path]:
    sessions_dir = codex_home / "sessions"
    if not sessions_dir.exists():
        return []
    return sorted(sessions_dir.rglob("*.jsonl"))


def parse_history_file(path: Path, max_event_chars: int) -> NormalizedSession | None:
    if not path.exists():
        return None

    events: list[NormalizedEvent] = []
    started_at: str | None = None
    with path.open("r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as exc:
                events.append(NormalizedEvent(
                    timestamp="",
                    kind="parse_error",
                    role=None,
                    text=f"JSON parse error at line {line_no}: {exc}",
                    metadata={"line_no": line_no},
                ))
                continue

            raw_ts = record.get("ts")
            timestamp = ""
            if isinstance(raw_ts, (int, float)):
                timestamp = dt.datetime.fromtimestamp(raw_ts, tz=dt.timezone.utc).isoformat()
                started_at = started_at or timestamp

            text = str(record.get("text") or "")
            events.append(NormalizedEvent(
                timestamp=timestamp,
                kind="history_prompt",
                role="user",
                text=truncate_text(redact_text(text), max_event_chars),
                metadata={
                    "record_type": "history",
                    "line_no": line_no,
                    "session_id": record.get("session_id"),
                },
            ))

    return NormalizedSession(
        session_id="codex-history",
        source_file=str(path),
        started_at=started_at,
        cwd=None,
        originator="codex-history",
        cli_version=None,
        model_provider=None,
        events=events,
    )


def parse_session_file(path: Path, max_event_chars: int) -> NormalizedSession | None:
    meta: dict[str, Any] = {}
    events: list[NormalizedEvent] = []

    with path.open("r", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as exc:
                events.append(NormalizedEvent(
                    timestamp="",
                    kind="parse_error",
                    role=None,
                    text=f"JSON parse error at line {line_no}: {exc}",
                    metadata={"line_no": line_no},
                ))
                continue

            if record.get("type") == "session_meta" and isinstance(record.get("payload"), dict):
                meta = record["payload"]
                continue

            event = extract_event(record, max_event_chars)
            if event is not None:
                events.append(event)

    session_id = str(meta.get("id") or path.stem)
    started_at = meta.get("timestamp")
    if not in_since_window(started_at, None):
        return None

    return NormalizedSession(
        session_id=session_id,
        source_file=str(path),
        started_at=str(started_at) if started_at else None,
        cwd=str(meta.get("cwd")) if meta.get("cwd") else None,
        originator=str(meta.get("originator")) if meta.get("originator") else None,
        cli_version=str(meta.get("cli_version")) if meta.get("cli_version") else None,
        model_provider=str(meta.get("model_provider")) if meta.get("model_provider") else None,
        events=events,
    )


def load_sessions(args: argparse.Namespace) -> list[NormalizedSession]:
    codex_home = Path(args.codex_home).expanduser()
    sessions: list[NormalizedSession] = []
    if not args.skip_history:
        history = parse_history_file(codex_home / "history.jsonl", args.max_event_chars)
        if history is not None and in_since_window(history.started_at, args.since_days):
            sessions.append(history)

    for path in session_files(codex_home):
        parsed = parse_session_file(path, args.max_event_chars)
        if parsed is None:
            continue
        if not in_since_window(parsed.started_at, args.since_days):
            continue
        sessions.append(parsed)
        if args.limit and len(sessions) >= args.limit:
            break
    return sessions


def write_export(sessions: Iterable[NormalizedSession], output: Path) -> tuple[int, int]:
    output.parent.mkdir(parents=True, exist_ok=True)
    session_count = 0
    event_count = 0
    with output.open("w", encoding="utf-8") as handle:
        for session in sessions:
            session_count += 1
            event_count += len(session.events)
            payload = dataclasses.asdict(session)
            payload["digest"] = session.digest()
            handle.write(json.dumps(payload, ensure_ascii=False, sort_keys=True) + "\n")
    return session_count, event_count


def import_to_langfuse(sessions: list[NormalizedSession], dry_run: bool) -> None:
    host = os.getenv("LANGFUSE_HOST", "http://localhost:3000")
    public_key = os.getenv("LANGFUSE_PUBLIC_KEY", "")
    secret_key = os.getenv("LANGFUSE_SECRET_KEY", "")

    if dry_run:
        return
    if not public_key or not secret_key:
        raise RuntimeError(
            "LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY are required for import. "
            "Create a project/API key in Langfuse and add them to tools/codex-langfuse/.env."
        )

    os.environ.setdefault("OTEL_BSP_MAX_QUEUE_SIZE", "200000")
    os.environ.setdefault("OTEL_BSP_MAX_EXPORT_BATCH_SIZE", "512")
    os.environ.setdefault("OTEL_BSP_SCHEDULE_DELAY", "200")

    try:
        from langfuse import Langfuse
    except ImportError as exc:
        raise RuntimeError(
            "The langfuse Python package is not installed. "
            "Run: ./scripts/langfuse-local.sh install-importer"
        ) from exc

    client = Langfuse(public_key=public_key, secret_key=secret_key, host=host)

    for session in sessions:
        trace_name = "codex-session"
        if session.cwd:
            trace_name = f"codex:{Path(session.cwd).name}"
        first_user = next((e.text for e in session.events if e.role == "user" and e.text), "")
        last_assistant = next((e.text for e in reversed(session.events) if e.role == "assistant" and e.text), "")
        metadata = {
            "source": "codex-local",
            "source_file": session.source_file,
            "cwd": session.cwd,
            "originator": session.originator,
            "cli_version": session.cli_version,
            "model_provider": session.model_provider,
            "digest": session.digest(),
            "event_count": len(session.events),
        }
        trace_id = hashlib.sha256(f"codex:{session.session_id}".encode("utf-8")).hexdigest()[:32]
        with client.start_as_current_span(
            trace_context={"trace_id": trace_id},
            name=trace_name,
            input=first_user or None,
            output=last_assistant or None,
            metadata=metadata,
        ):
            client.update_current_trace(
                name=trace_name,
                user_id=os.getenv("USER"),
                session_id=session.session_id,
                input=first_user or None,
                output=last_assistant or None,
                metadata=metadata,
                tags=["codex", "local-import"],
            )
            for index, event in enumerate(session.events):
                client.create_event(
                    name=f"{index:04d}:{event.kind}",
                    input=event.text if event.role == "user" else None,
                    output=event.text if event.role == "assistant" else None,
                    metadata={
                        **event.metadata,
                        "role": event.role,
                        "timestamp": event.timestamp,
                        "text": event.text if event.role not in {"user", "assistant"} else None,
                    },
                )
                if index and index % 500 == 0:
                    client.flush()
        client.flush()
    client.flush()


def print_summary(sessions: list[NormalizedSession], output: Path, dry_run: bool) -> None:
    event_count = sum(len(session.events) for session in sessions)
    print(f"sessions={len(sessions)} events={event_count} export={output}")
    if sessions:
        newest = sessions[-1]
        print(f"latest_session={newest.session_id} cwd={newest.cwd} started_at={newest.started_at}")
    if dry_run:
        print("dry_run=true; no data was sent to Langfuse")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--codex-home", default=str(DEFAULT_CODEX_HOME))
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT))
    parser.add_argument("--limit", type=int, default=0, help="maximum sessions to process")
    parser.add_argument("--since-days", type=int, default=None, help="only process sessions started within N days")
    parser.add_argument("--max-event-chars", type=int, default=20000)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--export-only", action="store_true")
    parser.add_argument("--skip-history", action="store_true", help="skip ~/.codex/history.jsonl")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    codex_home = Path(args.codex_home).expanduser()
    if not codex_home.exists():
        print(f"Codex home not found: {codex_home}", file=sys.stderr)
        return 1

    sessions = load_sessions(args)
    output = Path(args.output).expanduser()
    write_export(sessions, output)
    print_summary(sessions, output, args.dry_run or args.export_only)

    if args.export_only:
        return 0

    try:
        import_to_langfuse(sessions, dry_run=args.dry_run)
    except Exception as exc:
        print(f"import failed: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

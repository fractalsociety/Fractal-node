# Local Codex Session Observability With Langfuse

This folder adds a local Langfuse stack and an importer for Codex session files.
It is meant for private local observability: browsing past sessions, prompts,
tool calls, outputs, and basic session metadata.

## What gets imported

The importer reads:

- `~/.codex/sessions/**/*.jsonl`
- optionally normalized exports under `tools/codex-langfuse/export/`

It does not read `~/.codex/auth.json`. Common token patterns such as GitHub
PATs, OpenAI-style keys, bearer tokens, and `token=...` / `secret=...` values
are redacted before export or import.

Review the generated JSONL export before importing highly sensitive sessions.

## Start Langfuse

```bash
./scripts/langfuse-local.sh init
./scripts/langfuse-local.sh up
```

Open `http://localhost:3000`. The generated `.env` uses Langfuse headless
initialization to create a local org, `Codex Sessions` project, API key, and
initial user on startup.

The local login and API keys are in `tools/codex-langfuse/.env`:

```bash
LANGFUSE_INIT_USER_EMAIL=...
LANGFUSE_INIT_USER_PASSWORD=...
LANGFUSE_PUBLIC_KEY=...
LANGFUSE_SECRET_KEY=...
```

The generated `.env` is gitignored.

## Import Codex sessions

Install the importer dependencies once:

```bash
./scripts/langfuse-local.sh install-importer
```

Check what will be processed:

```bash
./scripts/langfuse-local.sh import --dry-run --limit 5
```

Create a reviewable normalized export only:

```bash
./scripts/langfuse-local.sh export --since-days 30
```

Import sessions into Langfuse:

```bash
./scripts/langfuse-local.sh import --since-days 30
```

## Operate the stack

```bash
./scripts/langfuse-local.sh status
./scripts/langfuse-local.sh logs
./scripts/langfuse-local.sh down
```

Docker volumes hold Postgres, ClickHouse, Redis, and MinIO data. Removing those
volumes deletes the local Langfuse history.

## Local ports

- Langfuse UI: `http://localhost:3000`
- MinIO API: `http://localhost:9090`
- MinIO console: `http://localhost:9091`

Change ports in `tools/codex-langfuse/.env` if they conflict with other local
services.

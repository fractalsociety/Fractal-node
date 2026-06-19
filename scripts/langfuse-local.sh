#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$ROOT_DIR/tools/codex-langfuse/.env"
EXAMPLE_ENV="$ROOT_DIR/tools/codex-langfuse/langfuse.env.example"
COMPOSE_FILE="$ROOT_DIR/docker-compose.langfuse.yml"
VENV_DIR="$ROOT_DIR/tools/codex-langfuse/.venv"

usage() {
  cat <<'USAGE'
Usage: ./scripts/langfuse-local.sh <command> [args]

Commands:
  init              Create tools/codex-langfuse/.env with local secrets
  up                Start local Langfuse
  down              Stop local Langfuse
  status            Show container status
  logs              Follow Langfuse logs
  install-importer  Create Python venv and install importer dependencies
  import [args]     Import Codex sessions into Langfuse
  export [args]     Normalize Codex sessions to JSONL only

Examples:
  ./scripts/langfuse-local.sh up
  ./scripts/langfuse-local.sh install-importer
  ./scripts/langfuse-local.sh import --dry-run --limit 5
  ./scripts/langfuse-local.sh import --since-days 14
USAGE
}

rand_hex() {
  openssl rand -hex "${1:-32}"
}

rand_base64() {
  openssl rand -base64 "${1:-32}" | tr -d '\n'
}

ensure_env() {
  if [[ -f "$ENV_FILE" ]]; then
    return
  fi

  cp "$EXAMPLE_ENV" "$ENV_FILE"
  NEXTAUTH_SECRET_VALUE="$(rand_base64 32)" \
    SALT_VALUE="$(rand_base64 16)" \
    ENCRYPTION_KEY_VALUE="$(rand_hex 32)" \
    LANGFUSE_PUBLIC_KEY_VALUE="lf_pk_codex_local_$(rand_hex 16)" \
    LANGFUSE_SECRET_KEY_VALUE="lf_sk_codex_local_$(rand_hex 32)" \
    LANGFUSE_INIT_USER_PASSWORD_VALUE="$(rand_base64 24)" \
    perl -0pi -e '
      s/replace-with-generated-secret/$ENV{NEXTAUTH_SECRET_VALUE}/g;
      s/replace-with-generated-salt/$ENV{SALT_VALUE}/g;
      s/replace-with-64-hex-chars/$ENV{ENCRYPTION_KEY_VALUE}/g;
      s/replace-with-generated-langfuse-public-key/$ENV{LANGFUSE_PUBLIC_KEY_VALUE}/g;
      s/replace-with-generated-langfuse-secret-key/$ENV{LANGFUSE_SECRET_KEY_VALUE}/g;
      s/replace-with-generated-local-password/$ENV{LANGFUSE_INIT_USER_PASSWORD_VALUE}/g;
    ' "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  echo "Created $ENV_FILE"
}

compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
    return
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    load_env
    docker-compose -f "$COMPOSE_FILE" "$@"
    return
  fi

  echo "Docker is installed, but Docker Compose is not available." >&2
  echo "Install the Docker Compose v2 plugin, then rerun this command." >&2
  exit 127
}

load_env() {
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    [[ "$line" != *=* ]] && continue
    export "$line"
  done < "$ENV_FILE"
}

case "${1:-}" in
  init)
    ensure_env
    ;;
  up)
    ensure_env
    compose up -d
    echo "Langfuse is starting at http://localhost:${LANGFUSE_PORT:-3000}"
    ;;
  down)
    ensure_env
    compose down
    ;;
  status)
    ensure_env
    compose ps
    ;;
  logs)
    ensure_env
    compose logs -f langfuse-web langfuse-worker
    ;;
  install-importer)
    python3 -m venv "$VENV_DIR"
    "$VENV_DIR/bin/pip" install -r "$ROOT_DIR/tools/codex-langfuse/requirements.txt"
    ;;
  import)
    shift
    ensure_env
    load_env
    if [[ ! -x "$VENV_DIR/bin/python" ]]; then
      echo "Importer venv not found. Run: ./scripts/langfuse-local.sh install-importer" >&2
      exit 1
    fi
    "$VENV_DIR/bin/python" "$ROOT_DIR/tools/codex-langfuse/import_codex_sessions.py" "$@"
    ;;
  export)
    shift
    ensure_env
    if [[ -x "$VENV_DIR/bin/python" ]]; then
      "$VENV_DIR/bin/python" "$ROOT_DIR/tools/codex-langfuse/import_codex_sessions.py" --export-only "$@"
    else
      python3 "$ROOT_DIR/tools/codex-langfuse/import_codex_sessions.py" --export-only "$@"
    fi
    ;;
  ""|-h|--help|help)
    usage
    ;;
  *)
    usage
    exit 1
    ;;
esac

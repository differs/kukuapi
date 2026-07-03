#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="$REPO_ROOT/tools"
BENCH_ROOT="${BENCH_ROOT:-/tmp/sub2api-forward-bench}"
COMPOSE_PROJECT="${COMPOSE_PROJECT:-sub2api-forward-bench}"
BENCH_SERVER_PORT="${BENCH_SERVER_PORT:-18081}"
BENCH_PG_PORT="${BENCH_PG_PORT:-15432}"
BENCH_REDIS_PORT="${BENCH_REDIS_PORT:-16379}"
BENCH_MOCK_PORT="${BENCH_MOCK_PORT:-19090}"
BENCH_REQUESTS_PER_LEVEL="${BENCH_REQUESTS_PER_LEVEL:-2000}"
BENCH_LEVELS="${BENCH_LEVELS:-1,8,32,64,128,256}"
BENCH_ENDPOINT="${BENCH_ENDPOINT:-responses}"
BENCH_MODEL="${BENCH_MODEL:-gpt-5.4}"
BENCH_UPSTREAM_DELAY_MS="${BENCH_UPSTREAM_DELAY_MS:-0}"
BENCH_USER_CONCURRENCY="${BENCH_USER_CONCURRENCY:-100000}"
BENCH_ACCOUNT_CONCURRENCY="${BENCH_ACCOUNT_CONCURRENCY:-100000}"

SERVER_BIN="$BENCH_ROOT/sub2api-bench-server"
SERVER_LOG="$BENCH_ROOT/server.log"
MOCK_LOG="$BENCH_ROOT/mock.log"
RESULTS_JSONL="$BENCH_ROOT/results.jsonl"
SUMMARY_JSON="$BENCH_ROOT/summary.json"
ENV_FILE="$BENCH_ROOT/compose.env"
DATA_DIR="$BENCH_ROOT/data"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 2
  }
}

cleanup() {
  if [[ "${KEEP_BENCH_STACK:-0}" != "1" ]]; then
    if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
      kill "$SERVER_PID" 2>/dev/null || true
      wait "$SERVER_PID" 2>/dev/null || true
    fi
    if [[ -n "${MOCK_PID:-}" ]] && kill -0 "$MOCK_PID" 2>/dev/null; then
      kill "$MOCK_PID" 2>/dev/null || true
      wait "$MOCK_PID" 2>/dev/null || true
    fi
    docker compose -p "$COMPOSE_PROJECT" -f "$TOOLS_DIR/docker-compose.forward-bench.yml" --env-file "$ENV_FILE" down -v >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

need_cmd docker
need_cmd jq
need_cmd curl
need_cmd python3
need_cmd go

rm -rf "$BENCH_ROOT"
mkdir -p "$BENCH_ROOT"
mkdir -p "$DATA_DIR"

cat >"$ENV_FILE" <<EOF
BENCH_PG_PORT=$BENCH_PG_PORT
BENCH_REDIS_PORT=$BENCH_REDIS_PORT
BENCH_POSTGRES_USER=sub2api
BENCH_POSTGRES_PASSWORD=sub2api-bench-pass
BENCH_POSTGRES_DB=sub2api
EOF

docker compose -p "$COMPOSE_PROJECT" -f "$TOOLS_DIR/docker-compose.forward-bench.yml" --env-file "$ENV_FILE" up -d --wait

MOCK_HOST=127.0.0.1 MOCK_PORT="$BENCH_MOCK_PORT" MOCK_DELAY_MS="$BENCH_UPSTREAM_DELAY_MS" \
  nohup python3 "$TOOLS_DIR/mock_openai_upstream.py" >"$MOCK_LOG" 2>&1 &
MOCK_PID=$!

for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:$BENCH_MOCK_PORT/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.2
done

(
  cd "$REPO_ROOT/backend"
  go build -o "$SERVER_BIN" ./cmd/server
)

AUTO_SETUP=true \
DATA_DIR="$DATA_DIR" \
SERVER_HOST=0.0.0.0 \
SERVER_PORT="$BENCH_SERVER_PORT" \
SERVER_MODE=debug \
RUN_MODE=standard \
DATABASE_HOST=127.0.0.1 \
DATABASE_PORT="$BENCH_PG_PORT" \
DATABASE_USER=sub2api \
DATABASE_PASSWORD=sub2api-bench-pass \
DATABASE_DBNAME=sub2api \
DATABASE_SSLMODE=disable \
REDIS_HOST=127.0.0.1 \
REDIS_PORT="$BENCH_REDIS_PORT" \
REDIS_PASSWORD= \
REDIS_DB=0 \
ADMIN_EMAIL=bench-admin@example.com \
ADMIN_PASSWORD=bench-admin-password \
JWT_SECRET=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
TOTP_ENCRYPTION_KEY=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789 \
SECURITY_URL_ALLOWLIST_ENABLED=false \
SECURITY_URL_ALLOWLIST_ALLOW_INSECURE_HTTP=true \
SECURITY_URL_ALLOWLIST_ALLOW_PRIVATE_HOSTS=true \
TZ=UTC \
  nohup "$SERVER_BIN" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:$BENCH_SERVER_PORT/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.3
done
curl -fsS "http://127.0.0.1:$BENCH_SERVER_PORT/health" >/dev/null

COMPOSE_BENCH=(docker compose -p "$COMPOSE_PROJECT" -f "$TOOLS_DIR/docker-compose.forward-bench.yml" --env-file "$ENV_FILE")
PSQL_BENCH=("${COMPOSE_BENCH[@]}" exec -T postgres psql -U sub2api -d sub2api -qAtc)

ADMIN_USER_ID="$("${PSQL_BENCH[@]}" "select id from users where email = 'bench-admin@example.com' limit 1;")"
if [[ -z "$ADMIN_USER_ID" ]]; then
  echo "failed to find bench admin user" >&2
  exit 1
fi
"${PSQL_BENCH[@]}" "update users set balance = 100000, updated_at = now() where id = $ADMIN_USER_ID;" >/dev/null
"${PSQL_BENCH[@]}" "update users set concurrency = $BENCH_USER_CONCURRENCY, updated_at = now() where id = $ADMIN_USER_ID;" >/dev/null

GROUP_ID="$("${PSQL_BENCH[@]}" "with existing as (select id from groups where name = 'bench-openai-forward' and deleted_at is null limit 1), inserted as (insert into groups (name, description, rate_multiplier, is_exclusive, status, platform, subscription_type, default_validity_days, claude_code_only, model_routing_enabled, mcp_xml_inject, supported_model_scopes, sort_order, allow_messages_dispatch, require_oauth_only, require_privacy_set, default_mapped_model, messages_dispatch_model_config, rpm_limit, created_at, updated_at) select 'bench-openai-forward', 'local forwarding benchmark group', 1, false, 'active', 'openai', 'standard', 30, false, false, true, '[\"claude\",\"gemini_text\",\"gemini_image\"]'::jsonb, 0, false, false, false, '', '{}'::jsonb, 0, now(), now() where not exists (select 1 from existing) returning id) select id from existing union all select id from inserted limit 1;")"

ACCOUNT_ID="$("${PSQL_BENCH[@]}" "with existing as (select id from accounts where name = 'bench-mock-openai' and deleted_at is null limit 1), inserted as (insert into accounts (name, platform, type, credentials, extra, concurrency, priority, rate_multiplier, status, schedulable, created_at, updated_at) select 'bench-mock-openai', 'openai', 'apikey', jsonb_build_object('api_key','bench-upstream-key','base_url','http://127.0.0.1:$BENCH_MOCK_PORT'), jsonb_build_object('openai_passthrough', true, 'openai_compact_mode', 'force_on'), $BENCH_ACCOUNT_CONCURRENCY, 1, 1.0, 'active', true, now(), now() where not exists (select 1 from existing) returning id) select id from existing union all select id from inserted limit 1;")"
"${PSQL_BENCH[@]}" "update accounts set concurrency = $BENCH_ACCOUNT_CONCURRENCY, updated_at = now() where id = $ACCOUNT_ID;" >/dev/null

"${PSQL_BENCH[@]}" "insert into account_groups (account_id, group_id, priority, created_at) values ($ACCOUNT_ID, $GROUP_ID, 1, now()) on conflict (account_id, group_id) do nothing;" >/dev/null

API_KEY="$(python3 - <<'PY'
import secrets
print("sk-bench-" + secrets.token_urlsafe(24))
PY
)"
KEY_NAME="bench-key-$(date +%s)"
"${PSQL_BENCH[@]}" "insert into api_keys (user_id, key, name, group_id, status, ip_whitelist, ip_blacklist, quota, quota_used, created_at, updated_at) values ($ADMIN_USER_ID, '$API_KEY', '$KEY_NAME', $GROUP_ID, 'active', '[]'::jsonb, '[]'::jsonb, 0, 0, now(), now());" >/dev/null

cat >"$BENCH_ROOT/runtime.env" <<EOF
API_ROOT=http://127.0.0.1:$BENCH_SERVER_PORT/api/v1
TARGET_URL=http://host.docker.internal:$BENCH_SERVER_PORT
API_KEY=$API_KEY
GROUP_ID=$GROUP_ID
ACCOUNT_ID=$ACCOUNT_ID
ADMIN_USER_ID=$ADMIN_USER_ID
EOF

case "$BENCH_ENDPOINT" in
  responses)
    TARGET_URL="http://host.docker.internal:$BENCH_SERVER_PORT/v1/responses"
    REQUEST_BODY="$(jq -cn --arg model "$BENCH_MODEL" '{model:$model,input:"bench"}')"
    ;;
  chat)
    TARGET_URL="http://host.docker.internal:$BENCH_SERVER_PORT/v1/chat/completions"
    REQUEST_BODY="$(jq -cn --arg model "$BENCH_MODEL" '{model:$model,messages:[{role:"user",content:"bench"}]}')"
    ;;
  compact)
    TARGET_URL="http://host.docker.internal:$BENCH_SERVER_PORT/v1/responses/compact"
    REQUEST_BODY="$(jq -cn --arg model "$BENCH_MODEL" '{model:$model,instructions:"You are a helpful coding assistant.",input:[{type:"message",role:"user",content:"bench"}]}')"
    ;;
  *)
    echo "unsupported BENCH_ENDPOINT=$BENCH_ENDPOINT" >&2
    exit 2
    ;;
esac

rm -f "$RESULTS_JSONL"
IFS=',' read -r -a LEVELS <<<"$BENCH_LEVELS"
for level in "${LEVELS[@]}"; do
  docker run --rm \
    --add-host=host.docker.internal:host-gateway \
    -v "$REPO_ROOT:/bench" \
    -w /bench/tools \
    golang:1.25 \
    env GO111MODULE=off go run ./forward_loadgen.go \
      --url "$TARGET_URL" \
      --method POST \
      --requests "$BENCH_REQUESTS_PER_LEVEL" \
      --concurrency "$level" \
      --timeout 30s \
      --header "Authorization: Bearer $API_KEY" \
      --header "Content-Type: application/json" \
      --body "$REQUEST_BODY" | tee -a "$RESULTS_JSONL"
done

python3 - "$RESULTS_JSONL" "$SUMMARY_JSON" <<'PY'
import json
import sys
from pathlib import Path

src = Path(sys.argv[1])
dst = Path(sys.argv[2])
raw = src.read_text(encoding="utf-8").strip()
decoder = json.JSONDecoder()
items = []
idx = 0
while idx < len(raw):
    while idx < len(raw) and raw[idx].isspace():
        idx += 1
    if idx >= len(raw):
        break
    obj, end = decoder.raw_decode(raw, idx)
    items.append(obj)
    idx = end

if not items:
    raise SystemExit("no benchmark results found")

eligible = [item for item in items if item.get("error_rate", 1) <= 0.001]
best = max(eligible or items, key=lambda item: item.get("requests_per_sec", 0))
summary = {
    "endpoint": best.get("url"),
    "runs": items,
    "best_run": best,
    "best_error_free_run": max(eligible, key=lambda item: item.get("requests_per_sec", 0)) if eligible else None,
}
dst.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps(summary, ensure_ascii=False, indent=2))
PY

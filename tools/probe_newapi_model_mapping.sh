#!/usr/bin/env bash
set -euo pipefail

DEFAULT_MAX_MODELS=10
DEFAULT_TIMEOUT=30
PREFERRED_MODELS=(
  gpt-5.5
  gpt-5.4
  gpt-5.4-mini
  gpt-5.3-codex
  gpt-5.2
  codex-auto-review
  gpt-4.1
  gpt-4.1-mini
  gpt-4o
  gpt-4o-mini
  o3
  o3-mini
  o4-mini
)
FALLBACK_MODELS=(
  gpt-5.5
  gpt-5.4
  gpt-5.4-mini
  gpt-5.3-codex
  gpt-5.2
  codex-auto-review
  gpt-4.1
  gpt-4.1-mini
  gpt-4o
  gpt-4o-mini
)

URL=""
KEY=""
CONN_FILE=""
CONN_JSON=""
MODELS_CSV=""
ALL_MODELS=0
MAX_MODELS="$DEFAULT_MAX_MODELS"
SKIP_COMPACT=0
TIMEOUT="$DEFAULT_TIMEOUT"
SOURCE=""

usage() {
  cat <<'EOF'
Usage:
  tools/probe_newapi_model_mapping.sh --url URL --key KEY
  tools/probe_newapi_model_mapping.sh --conn-file conn.json
  printf '%s\n' '{"_type":"newapi_channel_conn","url":"https://relay","key":"sk-..."}' | \
    tools/probe_newapi_model_mapping.sh

Options:
  --url URL
  --key KEY
  --conn-file PATH
  --conn-json JSON
  --models a,b,c
  --all-models
  --max-models N
  --skip-compact
  --timeout SECONDS
  -h, --help
EOF
}

die() {
  printf '%s\n' "$*" >&2
  exit 2
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

normalize_base_url() {
  local value
  value="$(printf '%s' "${1:-}" | tr -d '\r' | sed 's/[[:space:]]*$//')"
  [[ -n "$value" ]] || die "missing url"
  [[ "$value" == *"://"* ]] || value="https://$value"
  value="${value%/}"
  [[ "$value" == */v1 ]] && value="${value%/v1}"
  printf '%s\n' "$value"
}

redact_key() {
  local value="${1:-}"
  local len=${#value}
  if (( len <= 10 )); then
    printf '%s\n' "$value"
    return
  fi
  printf '%s...%s\n' "${value:0:6}" "${value:len-4:4}"
}

string_in_array() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

dedupe_lines() {
  awk 'NF && !seen[$0]++'
}

model_looks_text_capable() {
  local value
  value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    *embedding*|*rerank*|*whisper*|*transcribe*|*transcription*|*speech*|*tts*|*moderation*|*gpt-image*|*dall-e*)
      return 1
      ;;
  esac
  return 0
}

truncate_note() {
  local value
  value="$(printf '%s' "${1:-}" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g; s/^ //; s/ $//')"
  local limit="${2:-72}"
  if (( ${#value} <= limit )); then
    printf '%s\n' "$value"
    return
  fi
  printf '%s...\n' "${value:0:limit-3}"
}

parse_conn_payload() {
  local payload="$1"
  local parsed_url parsed_key
  parsed_url="$(jq -r '.url // empty' <<<"$payload")"
  parsed_key="$(jq -r '.key // empty' <<<"$payload")"
  [[ -n "$parsed_url" && -n "$parsed_key" ]] || die "connection json must contain url and key"
  URL="$(normalize_base_url "$parsed_url")"
  KEY="$parsed_key"
}

load_connection() {
  if [[ -n "$URL" && -n "$KEY" ]]; then
    URL="$(normalize_base_url "$URL")"
    SOURCE="cli_args"
    return
  fi
  if [[ -n "$CONN_FILE" ]]; then
    parse_conn_payload "$(cat "$CONN_FILE")"
    SOURCE="file:$CONN_FILE"
    return
  fi
  if [[ -n "$CONN_JSON" ]]; then
    parse_conn_payload "$CONN_JSON"
    SOURCE="cli_json"
    return
  fi
  if [[ -n "${NEWAPI_URL:-}" && -n "${NEWAPI_KEY:-}" ]]; then
    URL="$(normalize_base_url "$NEWAPI_URL")"
    KEY="$NEWAPI_KEY"
    SOURCE="env"
    return
  fi
  if [[ ! -t 0 ]]; then
    local payload
    payload="$(cat)"
    if [[ -n "$payload" ]]; then
      exec </dev/null
      parse_conn_payload "$payload"
      SOURCE="stdin_json"
      return
    fi
  fi
  die "provide --url/--key, --conn-file, --conn-json, env vars, or stdin json"
}

http_request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local header_file body_file err_file
  header_file="$(mktemp)"
  body_file="$(mktemp)"
  err_file="$(mktemp)"
  local status rc=0
  local -a cmd=(
    curl -sS -L -X "$method"
    -D "$header_file"
    -o "$body_file"
    -m "$TIMEOUT"
    -w '%{http_code}'
    -H "Authorization: Bearer $KEY"
    -H 'Accept: application/json'
    -H 'User-Agent: sub2api-newapi-model-probe/1.0'
  )
  if [[ -n "$body" ]]; then
    cmd+=(-H 'Content-Type: application/json' --data "$body")
  fi
  cmd+=("$url")
  status="$("${cmd[@]}" </dev/null 2>"$err_file")" || rc=$?
  if (( rc != 0 )); then
    printf '0\n%s\n%s\n' "$err_file" "$body_file"
    rm -f "$header_file"
    return
  fi
  printf '%s\n%s\n%s\n' "$status" "$header_file" "$body_file"
  rm -f "$err_file"
}

json_field_or_empty() {
  local expr="$1"
  local file="$2"
  jq -r "$expr // empty" "$file" 2>/dev/null || true
}

collect_probe_line() {
  local endpoint="$1"
  local model="$2"
  local compact="$3"
  local url="$4"
  local body="$5"
  local status tmp1 tmp2
  mapfile -t _resp < <(http_request POST "$url" "$body")
  status="${_resp[0]}"
  tmp1="${_resp[1]}"
  tmp2="${_resp[2]}"

  local returned_model response_status reasoning_effort reasoning_tokens output_text error_code error_message note mapped
  returned_model="$(json_field_or_empty '.model' "$tmp2")"
  response_status="$(json_field_or_empty '.status' "$tmp2")"
  reasoning_effort="$(json_field_or_empty '.reasoning.effort' "$tmp2")"
  reasoning_tokens="$(json_field_or_empty '.usage.output_tokens_details.reasoning_tokens' "$tmp2")"
  output_text="$(jq -r '([.output[]? | select(.type=="message") | .content[]? | select(.type=="output_text") | .text] | join("\n")) // empty' "$tmp2" 2>/dev/null || true)"
  error_code="$(json_field_or_empty '.error.code' "$tmp2")"
  error_message="$(jq -r '(.error.message // .message // empty)' "$tmp2" 2>/dev/null || true)"

  mapped="no"
  if [[ -n "$returned_model" && "$returned_model" != "$model" ]]; then
    mapped="yes"
  fi

  if [[ "$compact" == "1" ]]; then
    if [[ "$status" =~ ^2[0-9][0-9]$ ]]; then
      note="${output_text:-${response_status:-ok}}"
      printf '%s\t%s\t%s\tsupported\t%s\n' \
        "$model" "$status" "${returned_model:--}" "$(truncate_note "$note")"
    else
      note="$error_code"
      [[ -n "$note" && -n "$error_message" ]] && note="$note: $error_message"
      [[ -z "$note" ]] && note="$error_message"
      printf '%s\t%s\t%s\tfailed\t%s\n' \
        "$model" "$status" "${returned_model:--}" "$(truncate_note "${note:-error}")"
    fi
  else
    note="${output_text:-$error_message}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$model" "$status" "${returned_model:--}" "$mapped" "${reasoning_effort:--}" "${reasoning_tokens:--}" "$(truncate_note "${note:--}")"
  fi

  rm -f "$tmp1" "$tmp2"
}

fetch_models() {
  local status tmp1 tmp2
  mapfile -t _resp < <(http_request GET "$URL/v1/models" "")
  status="${_resp[0]}"
  tmp1="${_resp[1]}"
  tmp2="${_resp[2]}"
  if [[ "$status" != "200" ]]; then
    DISCOVERY_ERROR="$(cat "$tmp1" 2>/dev/null || true)"
    rm -f "$tmp1" "$tmp2"
    return
  fi
  mapfile -t DISCOVERED_MODELS < <(jq -r '.data[]?.id // empty' "$tmp2" 2>/dev/null | dedupe_lines)
  DISCOVERY_ERROR=""
  rm -f "$tmp1" "$tmp2"
}

select_probe_models() {
  PROBE_MODELS=()
  if [[ -n "$MODELS_CSV" ]]; then
    local item
    while IFS= read -r item; do
      [[ -n "$item" ]] && PROBE_MODELS+=("$item")
    done < <(tr ',' '\n' <<<"$MODELS_CSV" | sed 's/^ *//; s/ *$//' | dedupe_lines)
    return
  fi

  local candidates=()
  if ((${#DISCOVERED_MODELS[@]} > 0)); then
    local model
    for model in "${DISCOVERED_MODELS[@]}"; do
      if model_looks_text_capable "$model"; then
        candidates+=("$model")
      fi
    done
  fi
  if ((${#candidates[@]} == 0)); then
    candidates=("${FALLBACK_MODELS[@]}")
  fi

  local ordered=()
  local preferred
  for preferred in "${PREFERRED_MODELS[@]}"; do
    if string_in_array "$preferred" "${candidates[@]}"; then
      ordered+=("$preferred")
    fi
  done
  local model
  for model in "${candidates[@]}"; do
    if ! string_in_array "$model" "${ordered[@]}"; then
      ordered+=("$model")
    fi
  done

  if (( ALL_MODELS )); then
    PROBE_MODELS=("${ordered[@]}")
    return
  fi
  local count=0
  for model in "${ordered[@]}"; do
    PROBE_MODELS+=("$model")
    ((count+=1))
    (( count >= MAX_MODELS )) && break
  done
}

while (($# > 0)); do
  case "$1" in
    --url)
      URL="${2:-}"
      shift 2
      ;;
    --key)
      KEY="${2:-}"
      shift 2
      ;;
    --conn-file)
      CONN_FILE="${2:-}"
      shift 2
      ;;
    --conn-json)
      CONN_JSON="${2:-}"
      shift 2
      ;;
    --models)
      MODELS_CSV="${2:-}"
      shift 2
      ;;
    --all-models)
      ALL_MODELS=1
      shift
      ;;
    --max-models)
      MAX_MODELS="${2:-}"
      shift 2
      ;;
    --skip-compact)
      SKIP_COMPACT=1
      shift
      ;;
    --timeout)
      TIMEOUT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

need_cmd curl
need_cmd jq
need_cmd column

load_connection
DISCOVERED_MODELS=()
DISCOVERY_ERROR=""
fetch_models
select_probe_models

responses_file="$(mktemp)"
compact_file="$(mktemp)"
aliases=()
compact_supported=()

for model in "${PROBE_MODELS[@]}"; do
  body="$(jq -cn --arg model "$model" '{model:$model,input:("Reply with exactly: MODEL-CHECK::" + $model)}')"
  line="$(collect_probe_line "/v1/responses" "$model" 0 "$URL/v1/responses" "$body")"
  printf '%s\n' "$line" >>"$responses_file"
  returned="$(cut -f3 <<<"$line")"
  mapped="$(cut -f4 <<<"$line")"
  if [[ "$mapped" == "yes" && "$returned" != "-" ]]; then
    aliases+=("$model -> $returned")
  fi
done

if (( ! SKIP_COMPACT )); then
  for model in "${PROBE_MODELS[@]}"; do
    body="$(jq -cn --arg model "$model" '{model:$model,instructions:"You are a helpful coding assistant.",input:[{type:"message",role:"user",content:("Reply with exactly: MODEL-CHECK::" + $model)}]}')"
    line="$(collect_probe_line "/v1/responses/compact" "$model" 1 "$URL/v1/responses/compact" "$body")"
    printf '%s\n' "$line" >>"$compact_file"
    result="$(cut -f4 <<<"$line")"
    if [[ "$result" == "supported" ]]; then
      compact_supported+=("$model")
    fi
  done
fi

printf 'Connection\n'
printf '  Base URL: %s\n' "$URL"
printf '  Key: %s\n' "$(redact_key "$KEY")"
printf '  Source: %s\n' "$SOURCE"
printf '\n'

printf 'Discovery\n'
if [[ -z "$DISCOVERY_ERROR" ]]; then
  printf '  /v1/models: ok\n'
else
  printf '  /v1/models: failed\n'
fi
printf '  Advertised models: %s\n' "${#DISCOVERED_MODELS[@]}"
if [[ -n "$DISCOVERY_ERROR" ]]; then
  printf '  Error: %s\n' "$(truncate_note "$DISCOVERY_ERROR" 120)"
fi
printf '  Probe models: %s\n' "$(IFS=', '; printf '%s' "${PROBE_MODELS[*]}")"
printf '\n'

printf 'Responses Probe\n'
{
  printf 'Requested\tHTTP\tReturned\tMapped\tEffort\tR.Tokens\tNote\n'
  cat "$responses_file"
} | column -t -s $'\t'
printf '\n'

if (( ! SKIP_COMPACT )); then
  printf 'Compact Probe\n'
  {
    printf 'Requested\tHTTP\tReturned\tResult\tNote\n'
    cat "$compact_file"
  } | column -t -s $'\t'
  printf '\n'
fi

printf 'Summary\n'
if ((${#aliases[@]} > 0)); then
  printf '  Aliases detected: %s\n' "$(IFS=', '; printf '%s' "${aliases[*]}")"
else
  printf '  Aliases detected: none\n'
fi
if (( SKIP_COMPACT )); then
  printf '  Compact supported: skipped\n'
elif ((${#compact_supported[@]} > 0)); then
  printf '  Compact supported: %s\n' "$(IFS=', '; printf '%s' "${compact_supported[*]}")"
else
  printf '  Compact supported: none\n'
fi

rm -f "$responses_file" "$compact_file"

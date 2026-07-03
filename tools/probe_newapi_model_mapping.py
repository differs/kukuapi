#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from typing import Any


DEFAULT_TIMEOUT_SECONDS = 30.0
DEFAULT_MAX_MODELS = 10
DEFAULT_REQUEST_PATH_MODELS = "/v1/models"
DEFAULT_REQUEST_PATH_RESPONSES = "/v1/responses"
DEFAULT_REQUEST_PATH_COMPACT = "/v1/responses/compact"
DEFAULT_USER_AGENT = "sub2api-newapi-model-probe/1.0"
DEFAULT_TRANSPORT_RETRIES = 3
FALLBACK_PROBE_MODELS = [
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.3-codex",
    "gpt-5.2",
    "codex-auto-review",
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4o",
    "gpt-4o-mini",
]
PREFERRED_MODEL_ORDER = [
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.3-codex",
    "gpt-5.2",
    "codex-auto-review",
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4o",
    "gpt-4o-mini",
    "o3",
    "o3-mini",
    "o4-mini",
]
NON_TEXT_MODEL_KEYWORDS = (
    "embedding",
    "rerank",
    "whisper",
    "transcribe",
    "transcription",
    "speech",
    "tts",
    "moderation",
    "gpt-image",
    "dall-e",
)


@dataclass
class ConnectionConfig:
    url: str
    key: str
    source: str


@dataclass
class HTTPResult:
    status: int
    headers: dict[str, str]
    body: bytes


@dataclass
class ProbeResult:
    endpoint: str
    requested_model: str
    http_status: int
    returned_model: str | None
    response_status: str | None
    reasoning_effort: str | None
    reasoning_tokens: int | None
    output_text: str | None
    error_code: str | None
    error_message: str | None
    request_id: str | None

    @property
    def mapped(self) -> bool:
        return bool(self.returned_model) and self.returned_model != self.requested_model


def normalize_base_url(raw: str) -> str:
    value = (raw or "").strip()
    if not value:
        raise ValueError("missing url")
    if "://" not in value:
        value = "https://" + value
    parsed = urllib.parse.urlsplit(value)
    if not parsed.scheme or not parsed.netloc:
        raise ValueError(f"invalid url: {raw}")
    path = parsed.path.rstrip("/")
    if path.endswith("/v1"):
        path = path[:-3]
    normalized = urllib.parse.urlunsplit(
        (parsed.scheme, parsed.netloc, path, "", "")
    ).rstrip("/")
    return normalized


def redact_key(key: str) -> str:
    value = (key or "").strip()
    if len(value) <= 10:
        return value
    return f"{value[:6]}...{value[-4:]}"


def parse_connection_payload(payload: str) -> ConnectionConfig:
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise ValueError(f"invalid connection json: {exc}") from exc
    if not isinstance(data, dict):
        raise ValueError("connection json must be an object")
    url = str(data.get("url") or "").strip()
    key = str(data.get("key") or "").strip()
    if not url or not key:
        raise ValueError("connection json must contain url and key")
    return ConnectionConfig(url=normalize_base_url(url), key=key, source="connection_json")


def load_connection_config(args: argparse.Namespace) -> ConnectionConfig:
    if args.url and args.key:
        return ConnectionConfig(
            url=normalize_base_url(args.url),
            key=args.key.strip(),
            source="cli_args",
        )

    if args.conn_file:
        with open(args.conn_file, "r", encoding="utf-8") as handle:
            config = parse_connection_payload(handle.read())
        config.source = f"file:{args.conn_file}"
        return config

    if args.conn_json:
        config = parse_connection_payload(args.conn_json)
        config.source = "cli_json"
        return config

    env_url = os.getenv("NEWAPI_URL", "").strip()
    env_key = os.getenv("NEWAPI_KEY", "").strip()
    if env_url and env_key:
        return ConnectionConfig(
            url=normalize_base_url(env_url),
            key=env_key,
            source="env",
        )

    if not sys.stdin.isatty():
        payload = sys.stdin.read().strip()
        if payload:
            config = parse_connection_payload(payload)
            config.source = "stdin_json"
            return config

    raise ValueError("provide --url/--key, --conn-file, --conn-json, env vars, or stdin json")


def join_url(base_url: str, path: str) -> str:
    return normalize_base_url(base_url) + path


def request_json(
    url: str,
    headers: dict[str, str],
    method: str = "GET",
    body: dict[str, Any] | None = None,
    timeout: float = DEFAULT_TIMEOUT_SECONDS,
) -> HTTPResult:
    payload = None
    req_headers = dict(headers)
    if body is not None:
        payload = json.dumps(body).encode("utf-8")
        req_headers.setdefault("Content-Type", "application/json")
    req = urllib.request.Request(url=url, data=payload, headers=req_headers, method=method)
    last_error: Exception | None = None
    for attempt in range(DEFAULT_TRANSPORT_RETRIES):
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return HTTPResult(
                    status=int(resp.status),
                    headers={k.lower(): v for k, v in resp.headers.items()},
                    body=resp.read(),
                )
        except urllib.error.HTTPError as exc:
            return HTTPResult(
                status=int(exc.code),
                headers={k.lower(): v for k, v in exc.headers.items()},
                body=exc.read(),
            )
        except (urllib.error.URLError, OSError) as exc:
            last_error = exc
            if attempt + 1 < DEFAULT_TRANSPORT_RETRIES:
                time.sleep(0.5 * (attempt + 1))
                continue
    if shutil.which("curl"):
        return request_json_via_curl(
            url=url,
            headers=req_headers,
            method=method,
            payload=payload,
            timeout=timeout,
            fallback_error=last_error,
        )
    message = f"transport error: {last_error}" if last_error else "transport error"
    return HTTPResult(
        status=0,
        headers={},
        body=json.dumps(
            {"error": {"code": "transport_error", "message": message}},
            ensure_ascii=False,
        ).encode("utf-8"),
    )


def request_json_via_curl(
    url: str,
    headers: dict[str, str],
    method: str,
    payload: bytes | None,
    timeout: float,
    fallback_error: Exception | None,
) -> HTTPResult:
    with tempfile.NamedTemporaryFile() as header_file, tempfile.NamedTemporaryFile() as body_file:
        cmd = [
            "curl",
            "-sS",
            "-L",
            "-X",
            method,
            "-D",
            header_file.name,
            "-o",
            body_file.name,
            "-m",
            str(max(1, int(timeout))),
            "-w",
            "%{http_code}",
        ]
        for key, value in headers.items():
            cmd.extend(["-H", f"{key}: {value}"])
        if payload is not None:
            cmd.extend(["--data-binary", "@-"])
        cmd.append(url)
        completed = subprocess.run(
            cmd,
            input=payload,
            capture_output=True,
            check=False,
        )
        if completed.returncode != 0:
            message = completed.stderr.decode("utf-8", errors="replace").strip()
            if not message:
                message = f"transport error: {fallback_error}" if fallback_error else "transport error"
            return HTTPResult(
                status=0,
                headers={},
                body=json.dumps(
                    {"error": {"code": "transport_error", "message": message}},
                    ensure_ascii=False,
                ).encode("utf-8"),
            )
        with open(header_file.name, "r", encoding="utf-8", errors="replace") as handle:
            header_text = handle.read()
        with open(body_file.name, "rb") as handle:
            response_body = handle.read()
    status, response_headers = parse_curl_headers(header_text)
    status_text = completed.stdout.decode("utf-8", errors="replace").strip()
    if status == 0 and status_text.isdigit():
        status = int(status_text)
    return HTTPResult(status=status, headers=response_headers, body=response_body)


def parse_curl_headers(raw_headers: str) -> tuple[int, dict[str, str]]:
    status = 0
    headers: dict[str, str] = {}
    for raw_line in raw_headers.splitlines():
        line = raw_line.rstrip("\r")
        if not line:
            continue
        if line.startswith("HTTP/"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                status = int(parts[1])
                headers = {}
            continue
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        headers[key.strip().lower()] = value.strip()
    return status, headers


def parse_json_body(body: bytes) -> dict[str, Any] | None:
    if not body:
        return None
    try:
        data = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None
    return data if isinstance(data, dict) else None


def extract_error(data: dict[str, Any] | None, body: bytes) -> tuple[str | None, str | None]:
    if isinstance(data, dict):
        err = data.get("error")
        if isinstance(err, dict):
            code = err.get("code")
            message = err.get("message")
            return stringify(code), stringify(message)
        if isinstance(err, str):
            return None, err
        message = data.get("message")
        if isinstance(message, str) and message.strip():
            return None, message.strip()
    text = body.decode("utf-8", errors="replace").strip()
    return None, (text or None)


def stringify(value: Any) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def extract_output_text(data: dict[str, Any] | None) -> str | None:
    if not isinstance(data, dict):
        return None
    output = data.get("output")
    if isinstance(output, list):
        for item in output:
            if not isinstance(item, dict):
                continue
            if item.get("type") != "message":
                continue
            content = item.get("content")
            if not isinstance(content, list):
                continue
            parts: list[str] = []
            for block in content:
                if isinstance(block, dict) and block.get("type") == "output_text":
                    text = stringify(block.get("text"))
                    if text:
                        parts.append(text)
            if parts:
                return "\n".join(parts)
    output_text = stringify(data.get("output_text"))
    if output_text:
        return output_text
    choices = data.get("choices")
    if isinstance(choices, list) and choices:
        first = choices[0]
        if isinstance(first, dict):
            message = first.get("message")
            if isinstance(message, dict):
                return stringify(message.get("content"))
    return None


def extract_probe_result(endpoint: str, requested_model: str, response: HTTPResult) -> ProbeResult:
    data = parse_json_body(response.body)
    error_code, error_message = extract_error(data, response.body)
    reasoning = data.get("reasoning") if isinstance(data, dict) else None
    usage = data.get("usage") if isinstance(data, dict) else None
    output_details = (
        usage.get("output_tokens_details")
        if isinstance(usage, dict)
        else None
    )
    request_id = (
        response.headers.get("x-oneapi-request-id")
        or response.headers.get("x-request-id")
        or response.headers.get("request-id")
    )
    return ProbeResult(
        endpoint=endpoint,
        requested_model=requested_model,
        http_status=response.status,
        returned_model=stringify(data.get("model")) if isinstance(data, dict) else None,
        response_status=stringify(data.get("status")) if isinstance(data, dict) else None,
        reasoning_effort=stringify(reasoning.get("effort")) if isinstance(reasoning, dict) else None,
        reasoning_tokens=(
            int(output_details["reasoning_tokens"])
            if isinstance(output_details, dict)
            and isinstance(output_details.get("reasoning_tokens"), int)
            else None
        ),
        output_text=extract_output_text(data),
        error_code=error_code,
        error_message=error_message,
        request_id=request_id,
    )


def model_looks_text_capable(model_id: str) -> bool:
    value = model_id.strip().lower()
    if not value:
        return False
    return not any(keyword in value for keyword in NON_TEXT_MODEL_KEYWORDS)


def choose_probe_models(
    discovered_models: list[str],
    explicit_models: list[str] | None,
    all_models: bool,
    max_models: int,
) -> list[str]:
    if explicit_models:
        return dedupe_preserve_order(explicit_models)

    candidates = [model for model in discovered_models if model_looks_text_capable(model)]
    if not candidates:
        candidates = FALLBACK_PROBE_MODELS[:]

    ordered: list[str] = []
    remaining = set(candidates)
    for preferred in PREFERRED_MODEL_ORDER:
        if preferred in remaining:
            ordered.append(preferred)
            remaining.remove(preferred)
    ordered.extend(sorted(remaining))

    if all_models:
        return ordered
    return ordered[:max_models]


def dedupe_preserve_order(items: list[str]) -> list[str]:
    seen: set[str] = set()
    output: list[str] = []
    for item in items:
        value = item.strip()
        if not value or value in seen:
            continue
        seen.add(value)
        output.append(value)
    return output


def fetch_models(base_url: str, key: str, timeout: float) -> tuple[list[str], str | None]:
    response = request_json(
        join_url(base_url, DEFAULT_REQUEST_PATH_MODELS),
        headers=build_base_headers(key),
        timeout=timeout,
    )
    data = parse_json_body(response.body)
    if response.status != 200:
        _, message = extract_error(data, response.body)
        return [], message or f"HTTP {response.status}"
    models_raw = data.get("data") if isinstance(data, dict) else None
    if not isinstance(models_raw, list):
        return [], "models response missing data list"
    models: list[str] = []
    for item in models_raw:
        if isinstance(item, dict):
            model_id = stringify(item.get("id"))
            if model_id:
                models.append(model_id)
    return dedupe_preserve_order(models), None


def build_base_headers(key: str) -> dict[str, str]:
    return {
        "Authorization": f"Bearer {key}",
        "Accept": "application/json",
        "User-Agent": DEFAULT_USER_AGENT,
    }


def build_responses_payload(model: str) -> dict[str, Any]:
    return {
        "model": model,
        "input": f"Reply with exactly: MODEL-CHECK::{model}",
    }


def build_compact_payload(model: str) -> dict[str, Any]:
    return {
        "model": model,
        "instructions": "You are a helpful coding assistant.",
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": f"Reply with exactly: MODEL-CHECK::{model}",
            }
        ],
    }


def probe_model(
    base_url: str,
    key: str,
    model: str,
    endpoint: str,
    compact: bool,
    timeout: float,
) -> ProbeResult:
    headers = build_base_headers(key)
    headers["OpenAI-Beta"] = "responses=experimental"
    if compact:
        headers["Originator"] = "codex_cli_rs"
        headers["Version"] = "0.0.0"
        headers["Session_ID"] = f"probe_compact_{model}"
        headers["Conversation_ID"] = f"probe_compact_{model}"
        payload = build_compact_payload(model)
    else:
        payload = build_responses_payload(model)
    response = request_json(
        join_url(base_url, endpoint),
        headers=headers,
        method="POST",
        body=payload,
        timeout=timeout,
    )
    return extract_probe_result(endpoint=endpoint, requested_model=model, response=response)


def truncate(value: str | None, limit: int = 72) -> str:
    if not value:
        return "-"
    if len(value) <= limit:
        return value
    return value[: limit - 3] + "..."


def format_bool(value: bool) -> str:
    return "yes" if value else "no"


def format_table(headers: list[str], rows: list[list[str]]) -> str:
    widths = [len(header) for header in headers]
    for row in rows:
        for idx, cell in enumerate(row):
            widths[idx] = max(widths[idx], len(cell))
    def build_line(values: list[str]) -> str:
        return "  ".join(value.ljust(widths[idx]) for idx, value in enumerate(values))
    parts = [build_line(headers), build_line(["-" * width for width in widths])]
    parts.extend(build_line(row) for row in rows)
    return "\n".join(parts)


def summarize_aliases(results: list[ProbeResult]) -> list[str]:
    aliases = []
    for result in results:
        if result.mapped and result.returned_model:
            aliases.append(f"{result.requested_model} -> {result.returned_model}")
    return aliases


def summarize_compact(results: list[ProbeResult]) -> tuple[list[str], list[str]]:
    supported = []
    unavailable = []
    for result in results:
        if 200 <= result.http_status < 300:
            supported.append(result.requested_model)
        else:
            unavailable.append(result.requested_model)
    return supported, unavailable


def build_report(
    config: ConnectionConfig,
    discovered_models: list[str],
    discovery_error: str | None,
    probe_models: list[str],
    standard_results: list[ProbeResult],
    compact_results: list[ProbeResult],
) -> dict[str, Any]:
    aliases = summarize_aliases(standard_results)
    compact_supported, compact_unsupported = summarize_compact(compact_results)
    return {
        "connection": {
            "base_url": config.url,
            "key_preview": redact_key(config.key),
            "source": config.source,
        },
        "discovery": {
            "models_endpoint_ok": discovery_error is None,
            "advertised_models": discovered_models,
            "error": discovery_error,
            "probe_models": probe_models,
        },
        "responses": [asdict(item) | {"mapped": item.mapped} for item in standard_results],
        "compact": [asdict(item) | {"mapped": item.mapped} for item in compact_results],
        "summary": {
            "aliases": aliases,
            "compact_supported": compact_supported,
            "compact_unsupported": compact_unsupported,
        },
    }


def print_text_report(report: dict[str, Any]) -> None:
    connection = report["connection"]
    discovery = report["discovery"]
    print("Connection")
    print(f"  Base URL: {connection['base_url']}")
    print(f"  Key: {connection['key_preview']}")
    print(f"  Source: {connection['source']}")
    print()

    print("Discovery")
    advertised_count = len(discovery["advertised_models"])
    print(f"  /v1/models: {'ok' if discovery['models_endpoint_ok'] else 'failed'}")
    print(f"  Advertised models: {advertised_count}")
    if discovery["error"]:
        print(f"  Error: {discovery['error']}")
    print(f"  Probe models: {', '.join(discovery['probe_models']) or '-'}")
    print()

    response_rows = []
    for item in report["responses"]:
        note = item["output_text"] or item["error_message"] or "-"
        response_rows.append(
            [
                item["requested_model"],
                str(item["http_status"]),
                item["returned_model"] or "-",
                format_bool(bool(item["mapped"])),
                item["reasoning_effort"] or "-",
                str(item["reasoning_tokens"]) if item["reasoning_tokens"] is not None else "-",
                truncate(note),
            ]
        )
    print("Responses Probe")
    print(
        format_table(
            ["Requested", "HTTP", "Returned", "Mapped", "Effort", "R.Tokens", "Note"],
            response_rows,
        )
    )
    print()

    compact_rows = []
    for item in report["compact"]:
        if 200 <= int(item["http_status"]) < 300:
            verdict = "supported"
            note = item["output_text"] or item["response_status"] or "ok"
        else:
            verdict = "failed"
            parts = [part for part in [item["error_code"], item["error_message"]] if part]
            note = ": ".join(parts) if parts else "error"
        compact_rows.append(
            [
                item["requested_model"],
                str(item["http_status"]),
                item["returned_model"] or "-",
                verdict,
                truncate(note),
            ]
        )
    print("Compact Probe")
    print(
        format_table(
            ["Requested", "HTTP", "Returned", "Result", "Note"],
            compact_rows,
        )
    )
    print()

    summary = report["summary"]
    print("Summary")
    aliases = summary["aliases"]
    print(f"  Aliases detected: {', '.join(aliases) if aliases else 'none'}")
    compact_supported = summary["compact_supported"]
    print(
        f"  Compact supported: {', '.join(compact_supported) if compact_supported else 'none'}"
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Probe a NewAPI-compatible upstream for model aliasing and compact support."
    )
    parser.add_argument("--url", help="Upstream base URL, with or without /v1")
    parser.add_argument("--key", help="API key")
    parser.add_argument("--conn-file", help="Path to a newapi_channel_conn JSON file")
    parser.add_argument("--conn-json", help="Inline newapi_channel_conn JSON")
    parser.add_argument(
        "--models",
        help="Comma-separated requested models to probe. Default is auto-discovery.",
    )
    parser.add_argument(
        "--all-models",
        action="store_true",
        help="Probe every text-capable model returned by /v1/models.",
    )
    parser.add_argument(
        "--max-models",
        type=int,
        default=DEFAULT_MAX_MODELS,
        help=f"Maximum auto-selected models when --all-models is not used (default: {DEFAULT_MAX_MODELS}).",
    )
    parser.add_argument(
        "--skip-compact",
        action="store_true",
        help="Skip /v1/responses/compact probes.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SECONDS,
        help=f"Per-request timeout in seconds (default: {DEFAULT_TIMEOUT_SECONDS}).",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print the full report as JSON.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    try:
        config = load_connection_config(args)
    except ValueError as exc:
        sys.stderr.write(f"{exc}\n")
        return 2

    explicit_models = None
    if args.models:
        explicit_models = dedupe_preserve_order(args.models.split(","))

    discovered_models, discovery_error = fetch_models(config.url, config.key, args.timeout)
    probe_models = choose_probe_models(
        discovered_models=discovered_models,
        explicit_models=explicit_models,
        all_models=args.all_models,
        max_models=max(1, args.max_models),
    )

    standard_results = [
        probe_model(
            base_url=config.url,
            key=config.key,
            model=model,
            endpoint=DEFAULT_REQUEST_PATH_RESPONSES,
            compact=False,
            timeout=args.timeout,
        )
        for model in probe_models
    ]
    compact_results: list[ProbeResult] = []
    if not args.skip_compact:
        compact_results = [
            probe_model(
                base_url=config.url,
                key=config.key,
                model=model,
                endpoint=DEFAULT_REQUEST_PATH_COMPACT,
                compact=True,
                timeout=args.timeout,
            )
            for model in probe_models
        ]

    report = build_report(
        config=config,
        discovered_models=discovered_models,
        discovery_error=discovery_error,
        probe_models=probe_models,
        standard_results=standard_results,
        compact_results=compact_results,
    )

    if args.json:
        json.dump(report, sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
    else:
        print_text_report(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

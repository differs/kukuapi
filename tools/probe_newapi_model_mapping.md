# NewAPI Model Mapping Probe

`tools/probe_newapi_model_mapping.sh` is the primary one-click probe for this repo. It answers two questions:

- Which requested model name is actually echoed back by `/v1/responses`
- Whether `/v1/responses/compact` works for the same requested model

If you need a Python implementation or machine-readable JSON output, `tools/probe_newapi_model_mapping.py` is also available. The shell script is the safer default when shell networking is more reliable than Python’s HTTP stack.

## Input Methods

1. CLI args:

```bash
bash tools/probe_newapi_model_mapping.sh \
  --url https://codeworksta.com \
  --key sk-xxxx
```

2. `newapi_channel_conn` JSON file:

```bash
bash tools/probe_newapi_model_mapping.sh --conn-file /path/to/conn.json
```

3. `newapi_channel_conn` JSON inline:

```bash
bash tools/probe_newapi_model_mapping.sh \
  --conn-json '{"_type":"newapi_channel_conn","url":"https://codeworksta.com","key":"sk-xxxx"}'
```

4. `newapi_channel_conn` JSON via stdin:

```bash
printf '%s\n' '{"_type":"newapi_channel_conn","url":"https://codeworksta.com","key":"sk-xxxx"}' \
  | bash tools/probe_newapi_model_mapping.sh
```

5. Environment variables:

```bash
export NEWAPI_URL=https://codeworksta.com
export NEWAPI_KEY=sk-xxxx
bash tools/probe_newapi_model_mapping.sh
```

## Common Flags

Probe a custom model list:

```bash
bash tools/probe_newapi_model_mapping.sh \
  --url https://codeworksta.com \
  --key sk-xxxx \
  --models gpt-5.5,gpt-5.4,gpt-5.3-codex
```

Probe every discovered text-capable model:

```bash
bash tools/probe_newapi_model_mapping.sh \
  --url https://codeworksta.com \
  --key sk-xxxx \
  --all-models
```

Skip compact checks:

```bash
bash tools/probe_newapi_model_mapping.sh \
  --url https://codeworksta.com \
  --key sk-xxxx \
  --skip-compact
```

Emit machine-readable JSON with the Python variant:

```bash
python3 tools/probe_newapi_model_mapping.py \
  --url https://codeworksta.com \
  --key sk-xxxx \
  --json
```

## Notes

- Default auto mode probes up to 10 models to keep cost bounded. Use `--all-models` to enumerate everything returned by `/v1/models`.
- In this repo’s sandboxed environment, `--conn-json` or `--conn-file` is the most stable way to pass credentials.
- The strongest external signal is the `model` field returned by `/v1/responses`. A relay can still forge that field, so treat the result as high-confidence inference rather than backend-proof.
- Compact probing uses the Codex-style headers expected by this repo’s OpenAI compact checks.

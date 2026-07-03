from __future__ import annotations

import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tools import probe_newapi_model_mapping as probe


class NormalizeBaseURLTests(unittest.TestCase):
    def test_strips_v1_suffix(self) -> None:
        self.assertEqual(
            probe.normalize_base_url("https://example.com/v1"),
            "https://example.com",
        )

    def test_preserves_custom_prefix(self) -> None:
        self.assertEqual(
            probe.normalize_base_url("https://example.com/openai"),
            "https://example.com/openai",
        )

    def test_adds_scheme(self) -> None:
        self.assertEqual(
            probe.normalize_base_url("example.com"),
            "https://example.com",
        )


class ConnectionPayloadTests(unittest.TestCase):
    def test_parses_newapi_channel_conn_payload(self) -> None:
        config = probe.parse_connection_payload(
            '{"_type":"newapi_channel_conn","url":"https://relay.example.com/v1","key":"sk-test"}'
        )
        self.assertEqual(config.url, "https://relay.example.com")
        self.assertEqual(config.key, "sk-test")


class ChooseProbeModelsTests(unittest.TestCase):
    def test_prefers_known_models_before_other_discovered_models(self) -> None:
        selected = probe.choose_probe_models(
            discovered_models=[
                "custom-text-1",
                "gpt-5.3-codex",
                "gpt-5.5",
                "gpt-5.4",
            ],
            explicit_models=None,
            all_models=False,
            max_models=3,
        )
        self.assertEqual(selected, ["gpt-5.5", "gpt-5.4", "gpt-5.3-codex"])

    def test_filters_non_text_models(self) -> None:
        selected = probe.choose_probe_models(
            discovered_models=["text-embedding-3-large", "gpt-5.4"],
            explicit_models=None,
            all_models=True,
            max_models=10,
        )
        self.assertEqual(selected, ["gpt-5.4"])

    def test_explicit_models_win(self) -> None:
        selected = probe.choose_probe_models(
            discovered_models=["gpt-5.4"],
            explicit_models=["foo", "foo", "bar"],
            all_models=False,
            max_models=1,
        )
        self.assertEqual(selected, ["foo", "bar"])


class ExtractProbeResultTests(unittest.TestCase):
    def test_extracts_mapping_and_reasoning_tokens(self) -> None:
        response = probe.HTTPResult(
            status=200,
            headers={"x-oneapi-request-id": "rid-1"},
            body=(
                b'{"model":"gpt-5.4","status":"completed","reasoning":{"effort":"medium"},'
                b'"usage":{"output_tokens_details":{"reasoning_tokens":23}},'
                b'"output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}'
            ),
        )
        result = probe.extract_probe_result("/v1/responses", "gpt-5.2", response)
        self.assertEqual(result.returned_model, "gpt-5.4")
        self.assertEqual(result.reasoning_effort, "medium")
        self.assertEqual(result.reasoning_tokens, 23)
        self.assertEqual(result.output_text, "OK")
        self.assertTrue(result.mapped)

    def test_extracts_error_message(self) -> None:
        response = probe.HTTPResult(
            status=503,
            headers={},
            body=(
                b'{"error":{"code":"model_not_found","message":"compact unavailable"}}'
            ),
        )
        result = probe.extract_probe_result("/v1/responses/compact", "gpt-5.4", response)
        self.assertEqual(result.error_code, "model_not_found")
        self.assertEqual(result.error_message, "compact unavailable")


class CurlHeaderParsingTests(unittest.TestCase):
    def test_uses_last_http_block(self) -> None:
        status, headers = probe.parse_curl_headers(
            "HTTP/1.1 200 Connection established\r\n\r\n"
            "HTTP/2 503\r\n"
            "Content-Type: application/json\r\n"
            "X-Oneapi-Request-Id: rid-2\r\n\r\n"
        )
        self.assertEqual(status, 503)
        self.assertEqual(headers["content-type"], "application/json")
        self.assertEqual(headers["x-oneapi-request-id"], "rid-2")


if __name__ == "__main__":
    unittest.main()

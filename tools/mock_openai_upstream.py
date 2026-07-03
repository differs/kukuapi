#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


HOST = os.getenv("MOCK_HOST", "0.0.0.0")
PORT = int(os.getenv("MOCK_PORT", "19090"))
DELAY_MS = int(os.getenv("MOCK_DELAY_MS", "0"))
MODEL = os.getenv("MOCK_MODEL", "gpt-5.4")
RESPONSE_TEXT = os.getenv("MOCK_RESPONSE_TEXT", "OK")


def maybe_delay() -> None:
    if DELAY_MS > 0:
        time.sleep(DELAY_MS / 1000)


def json_bytes(payload: dict) -> bytes:
    return json.dumps(payload, ensure_ascii=False).encode("utf-8")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/health":
            self.respond_json(HTTPStatus.OK, {"status": "ok"})
            return
        if self.path == "/v1/models":
            self.respond_json(
                HTTPStatus.OK,
                {
                    "object": "list",
                    "data": [
                        {
                            "id": MODEL,
                            "object": "model",
                            "owned_by": "mock",
                            "supported_endpoint_types": ["openai"],
                        }
                    ],
                },
            )
            return
        self.respond_json(HTTPStatus.NOT_FOUND, {"error": {"message": "not found"}})

    def do_POST(self) -> None:  # noqa: N802
        content_length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(content_length) if content_length > 0 else b"{}"
        try:
            request = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            request = {}

        maybe_delay()

        if self.path == "/v1/responses" or self.path == "/v1/responses/compact":
            self.respond_json(
                HTTPStatus.OK,
                {
                    "id": "resp_mock_123",
                    "object": "response",
                    "status": "completed",
                    "model": request.get("model") or MODEL,
                    "output": [
                        {
                            "id": "msg_mock_123",
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [
                                {
                                    "type": "output_text",
                                    "text": RESPONSE_TEXT,
                                    "annotations": [],
                                    "logprobs": [],
                                }
                            ],
                        }
                    ],
                    "reasoning": {"effort": "none", "summary": None},
                    "usage": {
                        "input_tokens": 8,
                        "output_tokens": 4,
                        "total_tokens": 12,
                        "input_tokens_details": {"cached_tokens": 0},
                        "output_tokens_details": {"reasoning_tokens": 0},
                    },
                },
            )
            return

        if self.path == "/v1/chat/completions":
            self.respond_json(
                HTTPStatus.OK,
                {
                    "id": "chatcmpl-mock-123",
                    "object": "chat.completion",
                    "model": request.get("model") or MODEL,
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {"role": "assistant", "content": RESPONSE_TEXT},
                        }
                    ],
                    "usage": {
                        "prompt_tokens": 8,
                        "completion_tokens": 4,
                        "total_tokens": 12,
                    },
                },
            )
            return

        self.respond_json(HTTPStatus.NOT_FOUND, {"error": {"message": "not found"}})

    def respond_json(self, status: HTTPStatus, payload: dict) -> None:
        body = json_bytes(payload)
        self.send_response(int(status))
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)


def main() -> None:
    server = ThreadingHTTPServer((HOST, PORT), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()

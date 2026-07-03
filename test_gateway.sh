#!/usr/bin/env bash
# kukuapi-rs 网关测试脚本
# 测试 Agnes → OpenAI Codex 格式转发

set -e

BASE_URL="${KUKUAPI_URL:-http://127.0.0.1:18081}"
API_KEY="cpk-gGzppK18mQHHXowct6kOeDhCQjasnIh3M9m4OYGYMLqtGB7w"

echo "=== 1. Health Check ==="
curl -s "$BASE_URL/health" && echo " OK"

echo ""
echo "=== 2. 模型列表 ==="
curl -s -H "Authorization: Bearer $API_KEY" "$BASE_URL/v1/models" | python3 -m json.tool 2>/dev/null || echo "(no models configured yet)"

echo ""
echo "=== 3. Claude Messages API (Anthropic 格式) ==="
curl -s -X POST "$BASE_URL/v1/messages" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $API_KEY" \
  -d '{
    "model": "claude-sonnet-4-5-20250929",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Say hello in one word"}]
  }' | python3 -m json.tool 2>/dev/null

echo ""
echo "=== 4. OpenAI Chat Completions (OpenAI 格式) ==="
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{
    "model": "gpt-5.4",
    "messages": [{"role": "user", "content": "Say hello in one word"}],
    "max_tokens": 100
  }' | python3 -m json.tool 2>/dev/null

echo ""
echo "=== 5. DeepSeek 格式 ==="
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{
    "model": "deepseek-v4-pro",
    "messages": [{"role": "user", "content": "Say hello in one word"}],
    "thinking": {"type": "enabled"},
    "reasoning_effort": "high"
  }' | python3 -m json.tool 2>/dev/null

echo ""
echo "=== 6. Agnes 格式 (携 metadata) ==="
curl -s -X POST "$BASE_URL/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{
    "model": "agnes-v1",
    "messages": [{"role": "user", "content": "Say hello in one word"}],
    "metadata": {"agent_id": "codex-test", "session_id": "test-001"},
    "agent_id": "codex-test"
  }' | python3 -m json.tool 2>/dev/null

echo ""
echo "=== 7. Usage (用量查询) ==="
curl -s -H "Authorization: Bearer $API_KEY" "$BASE_URL/v1/usage" | python3 -m json.tool 2>/dev/null

echo ""
echo "=== 全部测试完成 ==="

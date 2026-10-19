#!/bin/sh
# Start the small model used by the live Apple Silicon integration test.
set -eu
environment=${VLLM_METAL_HOME:-"$HOME/.venv-vllm-metal"}
if [ ! -x "$environment/bin/vllm" ]; then
    echo "Install vLLM-Metal first: https://github.com/vllm-project/vllm-metal#installation" >&2
    exit 1
fi
export VLLM_NO_USAGE_STATS=1 HF_HUB_DISABLE_TELEMETRY=1
exec "$environment/bin/vllm" serve "${NANO_CODEX_LOCAL_MODEL:-mlx-community/Qwen3-0.6B-4bit}" \
    --host 127.0.0.1 --port "${NANO_CODEX_LOCAL_PORT:-18000}" \
    --enable-auto-tool-choice --tool-call-parser hermes --reasoning-parser qwen3 \
    --default-chat-template-kwargs '{"enable_thinking":false}' \
    --max-model-len 8192 --gpu-memory-utilization 0.25

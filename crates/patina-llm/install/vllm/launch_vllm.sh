#!/usr/bin/env bash
set -eu

# Example local launch script for the first PATINA vLLM lane.
# Adjust host, port, model id, and GPU settings for the serving machine.

MODEL_ID="${MODEL_ID:-google/gemma-3-4b-it}"
HOST="${HOST:-0.0.0.0}"
PORT="${PORT:-8000}"
API_KEY="${API_KEY:-patina-local-dev}"

exec vllm serve "${MODEL_ID}" \
  --host "${HOST}" \
  --port "${PORT}" \
  --api-key "${API_KEY}" \
  --dtype auto \
  --generation-config vllm

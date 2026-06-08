# vLLM Installation Lane

This directory is the first operational lane for `patina-llm`.

The target shape is:

1. one controlled `vLLM` server process
2. one explicit environment file
3. one explicit launch script
4. one PATINA-facing config file

## Recommended First Topology

- serving host: the machine with the GPU and model weights
- PATINA host: the machine running `patina-tui` or other PATINA tools
- network contract: PATINA calls the vLLM server over HTTP on a known base URL

## Files Here

- `launch_vllm.sh`
  example launch command for a local vLLM server
- `vllm.env.example`
  explicit environment variables for host, model, and API key
- `patina-llm.vllm.toml`
  example PATINA-side provider config

## Why vLLM First

- stronger serving posture for larger models and higher concurrency
- explicit support for Gemma 3 family models
- clear path to batching and multi-GPU growth

## Current Starting Model

- `google/gemma-3-4b-it`

Move to `google/gemma-3-12b-it` or `google/gemma-3-27b-it` only when the serving node is ready for
the memory and throughput tradeoff.

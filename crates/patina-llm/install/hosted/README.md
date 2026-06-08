# Hosted Provider Lane

This directory documents the non-local provider lane for `patina-llm`.

It is not the default deployment posture.

The active first milestone remains:

- self-hosted `vLLM`

But the core config model now explicitly leaves room for:

- OpenAI-hosted models
- Anthropic Claude models
- later hosted providers with the same assistant-facing trait

## Why Keep This Lane

- some assistant use cases may need frontier hosted models
- the TUI should not need a structural rewrite to switch between local and hosted routes
- policy, cost, and data-governance decisions can stay outside the core provider trait

## Current Posture

- `OpenAI` and `Anthropic` are represented as disabled future config slots
- there is no live HTTP adapter for them yet
- their presence in config is intentional so route selection becomes a stable first-class concept

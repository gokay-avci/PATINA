# patina-llm

`patina-llm` is the LLM infrastructure crate for PATINA hosts.

Current stance:

- primary provider: `vLLM`
- primary deployment shape: locally controlled server, potentially on another machine
- future provider slot: `Ollama`
- future hosted slots: `OpenAI` and `Anthropic Claude`
- future tool/context bridge: `MCP`, but not as the model-serving layer

## Folder Layout

- `src/`
  typed config, provider traits, and provider-specific modules
- `install/vllm/`
  the active installation lane for the first milestone
- `install/ollama/`
  reserved notes for a later provider addition
- `install/hosted/`
  notes for non-local provider routes

## Immediate Scope

This crate currently establishes:

- a vLLM-first provider/config model
- explicit future slots for hosted providers such as OpenAI and Claude
- a stable trait seam for assistant-facing gateways
- a clear on-disk installation layout inside `crates/patina-llm/`
- a real blocking vLLM chat adapter using the vLLM chat-completions HTTP route
- TOML config loading for provider stacks

This crate now has a real vLLM HTTP path. Hosted and Ollama adapters remain future additions.

The next slice should add:

1. integration from `patina-tui` assistant state into this crate
2. a small connectivity probe for provider health checks
3. hosted-provider adapters only if and when policy and cost posture justify them
4. Ollama parity once the vLLM lane is exercised

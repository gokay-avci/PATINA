# Ollama Future Lane

This directory is intentionally reserved, not active.

Reason:

- the current PATINA assistant milestone is being shaped around a vLLM-first serving architecture
- Ollama remains a valid later provider if the operator experience becomes more important than the
  broader serving posture

When Ollama is added, it should mirror the same structure used by `install/vllm/`:

1. explicit launch notes
2. explicit environment file
3. explicit PATINA config example
4. provider implementation in `src/ollama.rs`

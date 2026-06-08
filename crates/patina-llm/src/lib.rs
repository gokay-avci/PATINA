#![forbid(unsafe_code)]

/*!
What this crate implements: the shared local-LLM control plane for PATINA hosts such as `patina-tui`.
Design basis: the first milestone targets a remote self-hosted vLLM deployment while keeping a
future Ollama provider slot available behind the same assistant-facing seam.
Assumption: installation and operational guidance should live inside `crates/patina-llm/` so the
runtime shape, launch scripts, and config examples stay discoverable together.
*/

pub mod config;
pub mod hosted;
pub mod ollama;
pub mod provider;
pub mod vllm;

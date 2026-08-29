//! Self-contained agent core: LLM backend, tool registry, and the tool-call
//! loop. The design absorbs the patterns of the reference agent core
//! (context assembly, tool specs, permission surface) as independent code:
//! same behavior, own expression.

pub mod agent;
pub mod llm;
pub mod tools;

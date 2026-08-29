//! The tool-call loop: assemble context, ask the backend, execute requested
//! tools, and repeat until the model answers without tools.

use anyhow::Result;

use crate::llm::{ChatBackend, Message};
use crate::tools::ToolRegistry;

/// One observable step of a turn, emitted in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Streaming assistant prose.
    Thinking(String),
    /// A tool invocation, with its status transition.
    ToolCall { name: String, status: String },
    /// The final answer.
    Answer(String),
}

pub struct Agent {
    backend: Box<dyn ChatBackend>,
    tools: ToolRegistry,
    max_tool_rounds: usize,
}

impl Agent {
    pub fn new(backend: Box<dyn ChatBackend>, tools: ToolRegistry) -> Self {
        Self {
            backend,
            tools,
            max_tool_rounds: 5,
        }
    }

    /// Run one turn and return the observable steps in order.
    pub async fn run_turn(&self, user_message: &str) -> Result<Vec<Step>> {
        let mut messages = vec![
            Message {
                role: "system".into(),
                content: "You are telos, the SSCCS execution agent. \
                          Use the provided tools when the task needs the \
                          environment; answer concisely."
                    .into(),
                ..Default::default()
            },
            Message {
                role: "user".into(),
                content: user_message.to_string(),
                ..Default::default()
            },
        ];

        let mut steps = Vec::new();

        for _ in 0..self.max_tool_rounds {
            let resp = self
                .backend
                .complete(&messages, &self.tools.specs())
                .await?;

            if let Some(content) = &resp.content {
                if !content.trim().is_empty() {
                    steps.push(Step::Thinking(content.clone()));
                }
            }

            if resp.tool_calls.is_empty() {
                let answer = resp
                    .content
                    .filter(|c| !c.trim().is_empty())
                    .unwrap_or_else(|| "Done.".to_string());
                steps.push(Step::Answer(answer));
                return Ok(steps);
            }

            // Execute every requested tool and feed the results back.
            let mut tool_results = Vec::new();
            for call in &resp.tool_calls {
                steps.push(Step::ToolCall {
                    name: call.name.clone(),
                    status: "running".into(),
                });
                let result = self
                    .tools
                    .run(&call.name, call.arguments.clone())
                    .await
                    .unwrap_or_else(|e| format!("error: {e}"));
                tool_results.push((call.clone(), result));
                steps.push(Step::ToolCall {
                    name: call.name.clone(),
                    status: "completed".into(),
                });
            }

            messages.push(Message {
                role: "assistant".into(),
                content: resp.content.clone().unwrap_or_default(),
                tool_calls: Some(resp.tool_calls.clone()),
                ..Default::default()
            });
            for (call, result) in tool_results {
                messages.push(Message {
                    role: "tool".into(),
                    content: result,
                    tool_call_id: Some(call.id.clone()),
                    name: Some(call.name.clone()),
                    ..Default::default()
                });
            }
        }

        steps.push(Step::Answer("Tool loop exhausted.".into()));
        Ok(steps)
    }
}

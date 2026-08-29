//! Tool registry: named tools with JSON specs for the model and an executor
//! over the local environment.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

/// A tool invocation to execute.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// One executable tool. The spec follows the OpenAI tool schema so the
/// model can select and invoke it.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn spec(&self) -> Value;
    async fn run(&self, args: Value) -> Result<String>;
}

pub struct ReadFile;

#[async_trait::async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }
    fn spec(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a text file from the workspace",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        })
    }
    async fn run(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("read_file: missing path"))?;
        tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("read {path}"))
    }
}

pub struct WriteFile;

#[async_trait::async_trait]
impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }
    fn spec(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write a text file, creating parent directories",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        })
    }
    async fn run(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("write_file: missing path"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow!("write_file: missing content"))?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("mkdir {parent:?}"))?;
        }
        tokio::fs::write(path, content)
            .await
            .with_context(|| format!("write {path}"))?;
        Ok(format!("wrote {} bytes to {path}", content.len()))
    }
}

pub struct ListDirectory;

#[async_trait::async_trait]
impl Tool for ListDirectory {
    fn name(&self) -> &str {
        "list_directory"
    }
    fn spec(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "list_directory",
                "description": "List entries in a directory",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        })
    }
    async fn run(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("list_directory: missing path"))?;
        let mut read = tokio::fs::read_dir(path)
            .await
            .with_context(|| format!("read_dir {path}"))?;
        let mut names = Vec::new();
        while let Some(entry) = read.next_entry().await? {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
        names.sort();
        Ok(if names.is_empty() {
            "(empty)".to_string()
        } else {
            names.join("\n")
        })
    }
}

/// Named tool lookup and execution.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn default_tools() -> Self {
        Self {
            tools: vec![
                Box::new(ReadFile),
                Box::new(WriteFile),
                Box::new(ListDirectory),
            ],
        }
    }

    pub fn specs(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.spec()).collect()
    }

    pub async fn run(&self, name: &str, args: Value) -> Result<String> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| anyhow!("unknown tool: {name}"))?;
        tool.run(args).await
    }
}

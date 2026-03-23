use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Push { content: String },
    Next,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    pub message: Option<String>,
    pub content: Option<String>,
    pub queue_len: usize,
    pub daemon_pid: u32,
}

impl Response {
    pub fn ok(queue_len: usize, daemon_pid: u32) -> Self {
        Self {
            ok: true,
            message: None,
            content: None,
            queue_len,
            daemon_pid,
        }
    }

    pub fn with_content(mut self, content: Option<String>) -> Self {
        self.content = content;
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn error(message: impl Into<String>, queue_len: usize, daemon_pid: u32) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
            content: None,
            queue_len,
            daemon_pid,
        }
    }
}

//! llm-engine: 大模型引擎（OpenAI 兼容 Chat Completions 客户端）
//!
//! 职责：构造 FEN 提示 → 请求 → 提取 UCCI → 合法性校验（≤2 轮）；
//! 超时/网络/非法着以错误返回，由 game-core 适配层负责降级到内置引擎。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use xiangqi_core::board::Board;
use xiangqi_core::types::Move;

/// 大模型配置（持久化于 settings.json，密钥不随存档导出）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// API 根地址，如 https://api.openai.com/v1（端点 = base_url + /chat/completions）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 请求超时秒数（默认 10）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
    /// 采样温度（默认 0.2 求稳定）
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_timeout() -> u32 {
    10
}
fn default_temperature() -> f32 {
    0.2
}

impl LlmConfig {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        LlmConfig {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            timeout_secs: default_timeout(),
            temperature: default_temperature(),
        }
    }
}

/// 大模型引擎错误
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("配置无效：{0}")]
    InvalidConfig(String),
    #[error("HTTP 错误：{0}")]
    Http(#[from] reqwest::Error),
    #[error("请求超时（>{0}s）")]
    Timeout(u32),
    #[error("服务端返回 {0}：{1}")]
    Status(u16, String),
    #[error("响应中未找到 UCCI 着法：{0}")]
    NoMoveInReply(String),
    #[error("着法格式非法：{0}")]
    BadFormat(String),
    #[error("着法非法（不在合法着集合）：{0}")]
    IllegalMove(String),
}

/// 大模型客户端
#[derive(Debug, Clone)]
pub struct LlmClient {
    config: LlmConfig,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MsgContent,
}

#[derive(Deserialize)]
struct MsgContent {
    content: String,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        LlmClient { config, http }
    }

    /// 提议着法（异步）。内部最多两轮：首轮非法着 → 追加提示重试一次。
    pub async fn propose_move(&self, board: &Board) -> Result<Move, LlmError> {
        let legal = board.legal_moves();
        if legal.is_empty() {
            return Err(LlmError::BadFormat("轮走方无合法着".into()));
        }
        let fen = board.to_fen();
        let side = match board.side_to_move {
            xiangqi_core::types::Color::Red => "red",
            xiangqi_core::types::Color::Black => "black",
        };

        let system = "你是中国象棋引擎。规则要点：红方先行；UCCI 记法为 4 字符（如 h2e2，\
                     前两字符为起点、后两字符为终点，文件 a-i、行 0-9）。\
                     你只能输出一步着法，格式为 UCCI，禁止任何解释或多余字符。";
        let user = format!("FEN: {fen}\n轮走方: {side}\n请给出最佳着法。");

        let first = self
            .request(&[msg("system", system), msg("user", &user)])
            .await?;
        match self.legalize(&first, &legal) {
            Ok(mv) => Ok(mv),
            Err(_) => {
                // 追加提示重试一次（仍非法则返回错误由上层降级）
                let hint =
                    format!("你的着法 {first} 非法，请重新输出一个合法着法，仍仅输出 UCCI。");
                let second = self
                    .request(&[
                        msg("system", system),
                        msg("user", &user),
                        msg("user", &hint),
                    ])
                    .await?;
                self.legalize(&second, &legal)
            }
        }
    }

    /// 同步便捷入口：在独立 OS 线程内建 current_thread runtime 执行，
    /// 因此可从任意上下文调用（含 tokio runtime 内），供同步 Engine trait 使用。
    pub fn propose_move_sync(&self, board: &Board) -> Result<Move, LlmError> {
        let client = self.clone();
        let board = board.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| LlmError::InvalidConfig(e.to_string()))?;
                    rt.block_on(client.propose_move(&board))
                })
                .join()
                .map_err(|_| LlmError::InvalidConfig("同步调用线程失败".into()))?
        })
    }

    async fn request(&self, messages: &[Message]) -> Result<String, LlmError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        if self.config.model.trim().is_empty() || self.config.base_url.trim().is_empty() {
            return Err(LlmError::InvalidConfig("base_url/model 不能为空".into()));
        }
        let body = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            temperature: self.config.temperature,
            max_tokens: 64,
        };
        let timeout = Duration::from_secs(self.config.timeout_secs.max(1) as u64);
        let resp = tokio::time::timeout(
            timeout,
            self.http
                .post(&url)
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| LlmError::Timeout(self.config.timeout_secs))??;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Status(status.as_u16(), text));
        }
        let parsed: ChatResponse = resp.json().await?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::NoMoveInReply("无 choices".into()))?;
        Ok(content)
    }

    /// 从回复文本提取 UCCI 并校验合法性
    fn legalize(&self, reply: &str, legal: &[Move]) -> Result<Move, LlmError> {
        let ucci = extract_ucci(reply)
            .ok_or_else(|| LlmError::NoMoveInReply(reply.chars().take(64).collect()))?;
        let mv = Move::from_ucci(&ucci).ok_or_else(|| LlmError::BadFormat(ucci.clone()))?;
        if legal.contains(&mv) {
            Ok(mv)
        } else {
            Err(LlmError::IllegalMove(ucci))
        }
    }
}

fn msg(role: &str, content: &str) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
    }
}

/// 提取首个 UCCI 4 字符子串（[a-i][0-9][a-i][0-9]），可容忍前后解释文本
fn extract_ucci(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 4 {
        return None;
    }
    for i in 0..=chars.len() - 4 {
        let w = &chars[i..i + 4];
        if is_file(w[0]) && w[1].is_ascii_digit() && is_file(w[2]) && w[3].is_ascii_digit() {
            return Some(w.iter().collect());
        }
    }
    None
}

fn is_file(c: char) -> bool {
    matches!(c, 'a'..='i')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain() {
        assert_eq!(extract_ucci("h2e2"), Some("h2e2".to_string()));
    }

    #[test]
    fn extract_with_explanation() {
        assert_eq!(
            extract_ucci("我建议 h2e2 这是最佳着法"),
            Some("h2e2".to_string())
        );
    }

    #[test]
    fn extract_embedded() {
        assert_eq!(extract_ucci("abch2e2xyz"), Some("h2e2".to_string()));
    }

    #[test]
    fn extract_none() {
        assert_eq!(extract_ucci("你好世界"), None);
        assert_eq!(extract_ucci("h2e"), None);
        assert_eq!(extract_ucci("h2e2x"), Some("h2e2".to_string()));
    }

    #[test]
    fn config_defaults() {
        let c = LlmConfig::new("https://x/v1", "k", "m");
        assert_eq!(c.timeout_secs, 10);
        assert_eq!(c.temperature, 0.2);
    }
}

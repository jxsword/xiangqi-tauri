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
    30
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
    /// 流式返回（SSE）：响应头即时返回，显著降低首 token 等待；
    /// 服务端不支持流式时会回退普通 JSON
    stream: bool,
    /// qwen3 系列：关闭思考链，避免长推理拖慢响应并占满 max_tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
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

/// SSE 流式分片（OpenAI 兼容 data: {...} 行）
#[derive(Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}
#[derive(Deserialize)]
struct SseChoice {
    delta: SseDelta,
}
#[derive(Deserialize)]
struct SseDelta {
    #[serde(default)]
    content: Option<String>,
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

        let system = concat!(
            "你是中国象棋引擎，只能输出一步着法。\n",
            "坐标规则：UCCI 记法为 4 字符，前两字符为起点、后两字符为终点；文件 a-i 从左到右（红方视角），行 0-9 从红方底线到黑方底线。\n",
            "示例：FEN: rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w，红方最佳着法为 b2e2（炮二平五）。\n",
            "硬性要求：1) 起点必须是你方棋子所在的格子；2) 整着必须符合中国象棋规则（兵卒只能前进、炮翻山吃子、马蹩脚、车走直线、相/仕不出九宫等）；3) 只能输出一步着法，仅 4 个字符，禁止任何解释、标点或额外文字。"
        );
        let user = format!("FEN: {fen}\n轮走方: {side}\n请给出最佳着法。");

        let first = self
            .request(&[msg("system", system), msg("user", &user)])
            .await?;
        match self.legalize(&first, &legal) {
            Ok(mv) => Ok(mv),
            Err(_) => {
                // 二轮纠错：把全部合法着列出，让模型从中选择（大幅提升命中率）
                let list: Vec<String> = legal.iter().map(|m| m.to_ucci()).collect();
                let hint = format!(
                    "你的着法 {first} 非法。以下是从当前局面全部合法着法中挑出的候选，请只从中选择一步并仅输出 4 字符 UCCI：\n{}",
                    list.join(" ")
                );
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

    /// 连通性测试：发一个最小 chat 请求，能正常返回即视为可用
    pub async fn test_connection(&self) -> Result<(), LlmError> {
        let _ = self.request(&[msg("user", "ping")]).await?;
        Ok(())
    }

    /// 同步便捷入口（同 propose_move_sync），供 Tauri 命令调用
    pub fn test_connection_sync(&self) -> Result<(), LlmError> {
        let client = self.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| LlmError::InvalidConfig(e.to_string()))?;
                    rt.block_on(client.test_connection())
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
            max_tokens: 512,
            stream: true,
            enable_thinking: Some(false),
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
        // 流式（SSE）与普通 JSON 统一处理
        let is_sse = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/event-stream"))
            .unwrap_or(false);
        if is_sse {
            self.read_sse(resp, timeout).await
        } else {
            let parsed: ChatResponse = resp.json().await?;
            parsed
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content)
                .ok_or_else(|| LlmError::NoMoveInReply("无 choices".into()))
        }
    }

    /// 解析 SSE 流（data: {...} 行，[DONE] 结束），拼接 delta.content
    async fn read_sse(
        &self,
        mut resp: reqwest::Response,
        timeout: Duration,
    ) -> Result<String, LlmError> {
        let mut content = String::new();
        let mut pending = String::new();
        loop {
            let chunk = tokio::time::timeout(timeout, resp.chunk())
                .await
                .map_err(|_| LlmError::Timeout(self.config.timeout_secs))??;
            match chunk {
                None => break,
                Some(bytes) => {
                    pending.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(pos) = pending.find('\n') {
                        let line = pending[..pos].trim().to_string();
                        pending.drain(..=pos);
                        if let Some(data) = line.strip_prefix("data:") {
                            let data = data.trim();
                            if data == "[DONE]" {
                                return Ok(content);
                            }
                            if let Ok(v) = serde_json::from_str::<SseChunk>(data) {
                                if let Some(c) = v.choices.first() {
                                    if let Some(d) = &c.delta.content {
                                        content.push_str(d);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
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

//! llm-engine: 大模型引擎（OpenAI 兼容 Chat Completions 客户端）
//!
//! 职责：构造 Prompt v2（坐标即清单语言 + ASCII 棋盘图 + 候选/全量清单）
//! → 流式 SSE 请求（空闲超时）→ 五层解析过滤 → 白名单校验（≤3 次尝试）；
//! 超时/网络/非法着以错误返回，由 game-core 适配层负责降级到内置引擎。
//!
//! 设计不变式（移植自 Flutter 版 HybridLlmMoveSource，勿破坏）：
//! 1. 大模型只在本地生成的白名单清单里选，输出格式与清单条目同构；
//! 2. 每次请求只有 system+user 两条消息，重试 = 追加反馈重发；
//! 3. 流式 SSE + 空闲超时（思考型模型兼容，持续吐字不误判）；
//! 4. 归一化过滤：剥代码围栏 / 删零宽字符 / 全角转半角 / 转小写；
//! 5. 参谋报告须用根节点全窗口评分（真实分差）才有否决意义。

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use xiangqi_core::board::Board;
use xiangqi_core::types::{coord, Color, Move, Piece, PieceKind};

/// 大模型配置（持久化于应用数据目录 llm-config.json）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    /// API 根地址，如 https://api.openai.com/v1（端点 = base_url + /chat/completions）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 请求空闲超时秒数（两次数据块最大间隔；总耗时上限 = ×4）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
    /// 采样温度（默认 0.2 求稳定）
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// 引擎参谋制：off=纯大模型 / candidate=引擎Top-K短名单 / gate=引擎护航否决
    #[serde(default = "default_advisor")]
    pub advisor: String,
    /// 参谋强度 0..=100（candidate 决定短名单条数 K=3+blend/20；gate 决定否决阈值 80+3.2×blend 厘兵）
    #[serde(default = "default_blend")]
    pub blend: u8,
}

fn default_timeout() -> u32 {
    30
}
fn default_temperature() -> f32 {
    0.2
}
fn default_advisor() -> String {
    "candidate".into()
}
fn default_blend() -> u8 {
    60
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
            advisor: default_advisor(),
            blend: default_blend(),
        }
    }

    /// candidate 模式：引擎 Top-K 短名单条数（3..=8）
    pub fn top_k(&self) -> usize {
        (3 + (self.blend as usize) / 20).clamp(3, 8)
    }

    /// gate 模式：否决阈值（厘兵）；blend=100 时不否决
    pub fn veto_threshold(&self) -> Option<i32> {
        if self.blend >= 100 {
            None
        } else {
            Some(80 + 32 * self.blend as i32 / 10)
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
    #[error("请求空闲超时（>{0}s）")]
    Timeout(u32),
    #[error("服务端返回 {0}：{1}")]
    Status(u16, String),
    #[error("响应中未找到 UCCI 着法：{0}")]
    NoMoveInReply(String),
    #[error("着法格式非法：{0}")]
    BadFormat(String),
    #[error("着法不在候选清单（非法/非白名单）：{0}")]
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
    stream: bool,
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
    #[serde(default)]
    error: Option<serde_json::Value>,
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

/// 单次"提议着法"的上下文（由 game-core 参谋适配层组装传入）。
/// 全部字段来自本地规则/引擎，零模型幻觉。
#[derive(Debug, Clone, Default)]
pub struct LlmRequestContext {
    /// ASCII 棋盘图（None 时内部生成）
    pub ascii_board: Option<String>,
    /// 对局着法 UCCI（旧→新顺序，最多保留最近 60 着）
    pub history_ucci: Vec<String>,
    /// 候选清单（ucci, 走子方视角引擎分）：candidate 模式使用（引擎 Top-K 短名单）
    pub candidate_moves: Vec<(String, i32)>,
    /// 全量合法着 UCCI：off/gate 模式的清单与白名单
    pub full_legal: Vec<String>,
    /// gate 参谋否决后再问的理由（Some 时追加【参谋否决】段）
    pub veto_reason: Option<String>,
    /// 循环重复警示（Some 时追加【警示】段）
    pub repetition_warning: Option<String>,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        LlmClient::with_proxy(config, true)
    }

    /// 禁用系统代理的客户端（集成测试连本机 mock 用；
    /// 避免系统 http_proxy 环境变量把 127.0.0.1 请求劫持到代理）
    pub fn new_no_proxy(config: LlmConfig) -> Self {
        LlmClient::with_proxy(config, false)
    }

    fn with_proxy(config: LlmConfig, use_system_proxy: bool) -> Self {
        let mut builder = reqwest::Client::builder();
        if !use_system_proxy {
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        LlmClient { config, http }
    }

    /// 当前配置（含参谋模式/强度），供 game-core 参谋适配层读取
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// 提议着法（异步）。off 基线：全量合法清单 + 二段式输出。
    pub async fn propose_move(&self, board: &Board) -> Result<Move, LlmError> {
        let legal = board.legal_moves();
        let full_legal: Vec<String> = legal.iter().map(|m| m.to_ucci()).collect();
        if full_legal.is_empty() {
            return Err(LlmError::BadFormat("轮走方无合法着".into()));
        }
        let ctx = LlmRequestContext {
            ascii_board: None,
            history_ucci: Vec::new(),
            candidate_moves: Vec::new(),
            full_legal,
            veto_reason: None,
            repetition_warning: None,
        };
        self.propose_move_advised(board, &ctx).await
    }

    /// 同步便捷入口，供 Tauri 命令/同步 Engine trait 调用
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

    /// 参谋制提议（候选短名单 / 护航 / 全量清单），≤3 次尝试
    pub async fn propose_move_advised(
        &self,
        board: &Board,
        ctx: &LlmRequestContext,
    ) -> Result<Move, LlmError> {
        if board.legal_moves().is_empty() {
            return Err(LlmError::BadFormat("轮走方无合法着".into()));
        }
        let candidate = !ctx.candidate_moves.is_empty();
        let pool: Vec<String> = if candidate {
            ctx.candidate_moves.iter().map(|(u, _)| u.clone()).collect()
        } else {
            ctx.full_legal.clone()
        };
        if pool.is_empty() {
            return Err(LlmError::InvalidConfig("白名单清单为空".into()));
        }

        let fen = board.to_fen();
        let side = match board.side_to_move {
            Color::Red => "红方",
            Color::Black => "黑方",
        };
        let ascii = ctx
            .ascii_board
            .clone()
            .unwrap_or_else(|| ascii_board(board));
        let system = system_v2(side, candidate);
        let base_user = build_user_v2(&fen, side, &ascii, ctx, candidate, board);

        let mut last_err: LlmError = LlmError::NoMoveInReply("无回复".into());
        for attempt in 0..3 {
            let user = if attempt == 0 {
                base_user.clone()
            } else {
                format!("{base_user}\n\n{}", retry_feedback(&last_err))
            };
            let reply = self
                .request(&[msg("system", system), msg("user", &user)])
                .await?; // 网络/超时错误直接返回（由上层决定降级）
            match self.legalize_pool(&reply, &pool) {
                Ok(mv) => return Ok(mv),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// 参谋制同步入口
    pub fn propose_move_advised_sync(
        &self,
        board: &Board,
        ctx: &LlmRequestContext,
    ) -> Result<Move, LlmError> {
        let client = self.clone();
        let board = board.clone();
        let ctx = ctx.clone();
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| LlmError::InvalidConfig(e.to_string()))?;
                    rt.block_on(client.propose_move_advised(&board, &ctx))
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
            max_tokens: 1024,
            stream: true,
            enable_thinking: Some(false),
        };
        let idle = Duration::from_secs(self.config.timeout_secs.max(1) as u64);
        let resp = tokio::time::timeout(
            idle,
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
            self.read_sse(resp, idle).await
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

    /// 解析 SSE 流：**空闲超时**（两次数据块最大间隔 idle，总耗时上限 idle×4），
    /// 持续吐字的思考型模型不会被误判超时。data:[DONE] 结束。
    async fn read_sse(
        &self,
        mut resp: reqwest::Response,
        idle: Duration,
    ) -> Result<String, LlmError> {
        let total_limit = idle.saturating_mul(4);
        let started = Instant::now();
        let mut content = String::new();
        let mut pending = String::new();
        loop {
            if started.elapsed() > total_limit {
                return Err(LlmError::Timeout(self.config.timeout_secs));
            }
            let chunk = tokio::time::timeout(idle, resp.chunk())
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
                            match serde_json::from_str::<SseChunk>(data) {
                                Ok(v) => {
                                    if let Some(err) = v.error {
                                        return Err(LlmError::Status(
                                            0,
                                            format!("SSE 错误：{err}"),
                                        ));
                                    }
                                    if let Some(c) = v.choices.first() {
                                        if let Some(d) = &c.delta.content {
                                            content.push_str(d);
                                        }
                                    }
                                }
                                Err(_) => {} // 非 JSON 数据行（心跳等）忽略
                            }
                        }
                    }
                }
            }
        }
        Ok(content)
    }

    /// 五层过滤：归一化 → 坐标提取 → Move 解析 → 白名单校验
    fn legalize_pool(&self, reply: &str, pool: &[String]) -> Result<Move, LlmError> {
        let ucci = extract_move_v2(reply)
            .ok_or_else(|| LlmError::NoMoveInReply(reply.chars().take(80).collect()))?;
        let mv = Move::from_ucci(&ucci).ok_or_else(|| LlmError::BadFormat(ucci.clone()))?;
        if pool.iter().any(|u| u == &ucci) {
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

/// system v2：坐标约定 + 白名单约束 + 两段式回复格式。
/// with_bucket_guide=true 用于候选模式（清单附引擎分档时引导选"最佳/均势"）。
fn system_v2(side: &str, with_bucket_guide: bool) -> &'static str {
    if with_bucket_guide {
        concat!(
            "你是中国象棋对弈引擎的着法接口，本局执红方或黑方。\n",
            "坐标约定：列用字母 a-i（从左到右，红方视角），行用数字 0-9（0 为红方底线、棋盘底部，9 为黑方底线、棋盘顶部）。着法格式为「起点-终点」，如 h2-e2。\n",
            "你只能从用户提供的「候选着法清单」中选择一步，禁止编造清单之外的着法。\n",
            "【回复格式（唯一允许的格式，共两段）】\n",
            "第一段以「分析:」开头，用一两句话（不超过 100 字）说明你的计划（进攻目标、需要提防的威胁）。\n",
            "最后一段为一行，形式为：\n",
            "着法: 起点-终点\n",
            "示例：着法: b2-e2\n",
            "候选着法清单中每条着法附有「—」后的引擎评估分档，请优先考虑评估为「最佳/均势」的着法，避免选择「大亏/致命」档的着法。"
        )
    } else {
        concat!(
            "你是中国象棋对弈引擎的着法接口，本局执红方或黑方。\n",
            "坐标约定：列用字母 a-i（从左到右，红方视角），行用数字 0-9（0 为红方底线、棋盘底部，9 为黑方底线、棋盘顶部）。着法格式为「起点-终点」，如 h2-e2。\n",
            "你只能从用户提供的「合法着法清单」中选择一步，禁止编造清单之外的着法。\n",
            "【回复格式（唯一允许的格式，共两段）】\n",
            "第一段以「分析:」开头，用一两句话（不超过 100 字）说明你的计划（进攻目标、需要提防的威胁）。\n",
            "最后一段为一行，形式为：\n",
            "着法: 起点-终点\n",
            "示例：着法: b2-e2"
        )
    }
}

/// user v2：FEN + ASCII 棋盘图 + 历史 + 清单（候选附分档/注解，全量附注解）+ 警示/否决段
fn build_user_v2(
    fen: &str,
    side: &str,
    ascii: &str,
    ctx: &LlmRequestContext,
    candidate: bool,
    board: &Board,
) -> String {
    let mut s = format!(
        "【当前局面 FEN】{fen}\n【棋盘图】\n{ascii}\n【轮走方】{side}（该方是你）\n"
    );

    if !ctx.history_ucci.is_empty() {
        let hist = ctx.history_ucci.iter().cloned().collect::<Vec<_>>().join(" ");
        s.push_str(&format!("【对局着法（最新在最后）】{hist}\n"));
    }

    if candidate {
        let best = ctx.candidate_moves.first().map(|(_, sc)| *sc).unwrap_or(0);
        let lines: Vec<String> = ctx
            .candidate_moves
            .iter()
            .map(|(u, sc)| {
                let ann = Move::from_ucci(u)
                    .and_then(|m| {
                        let a = annotate_move(board, m);
                        if a.is_empty() {
                            None
                        } else {
                            Some(a)
                        }
                    })
                    .unwrap_or_default();
                format!("{u}{ann} — {}", bucket_cp(best - sc))
            })
            .collect();
        s.push_str(&format!(
            "【候选着法清单（共 {} 条，由本地引擎选出，必须从中选择一条；「—」后为引擎评估分档）】\n{}\n",
            lines.len(),
            lines.join("\n")
        ));
    } else {
        let lines: Vec<String> = ctx
            .full_legal
            .iter()
            .map(|u| {
                let ann = Move::from_ucci(u)
                    .and_then(|m| {
                        let a = annotate_move(board, m);
                        if a.is_empty() {
                            None
                        } else {
                            Some(a)
                        }
                    })
                    .unwrap_or_default();
                format!("{u}{ann}")
            })
            .collect();
        s.push_str(&format!(
            "【合法着法清单（共 {} 条，必须从中选择一条）】\n{}\n",
            lines.len(),
            lines.join("\n")
        ));
    }

    if let Some(w) = &ctx.repetition_warning {
        s.push_str(&format!("【警示】{w}\n"));
    }
    if let Some(v) = &ctx.veto_reason {
        s.push_str(&format!("【参谋否决】{v}\n"));
    }

    s.push_str("【输出】先输出「分析:」段，最后一行输出「着法: 起点-终点」");
    s
}

/// 重试反馈：回显无效着法与原因，追加到 user 末尾后重发
fn retry_feedback(err: &LlmError) -> String {
    let (bad, reason) = match err {
        LlmError::NoMoveInReply(r) => ("（无法解析）".to_string(), r.clone()),
        LlmError::BadFormat(u) => (u.clone(), "格式非法".to_string()),
        LlmError::IllegalMove(u) => (u.clone(), "不在候选清单中".to_string()),
        other => ("（未知错误）".to_string(), other.to_string()),
    };
    format!(
        "你上一次的回复的着法 {bad} 无效（{reason}）。着法必须取自候选着法清单。请重新回答：先「分析:」一两句，最后一行「着法: 起点-终点」。"
    )
}

/// 评估分档（相对最佳分差 loss ≥ 0，厘兵）
pub fn bucket_cp(loss: i32) -> &'static str {
    if loss <= 30 {
        "最佳/均势"
    } else if loss <= 100 {
        "略亏"
    } else if loss <= 250 {
        "明显亏（约半子）"
    } else if loss <= 600 {
        "大亏（丢一马/一炮级）"
    } else {
        "致命（丢车/被将杀级）"
    }
}

/// ASCII 棋盘图（10 行，红大写/黑小写 FEN 字母，行号 0-9、列标 a-i）
pub fn ascii_board(board: &Board) -> String {
    let mut s = String::from("    a b c d e f g h i\n");
    for rank in (0..10u8).rev() {
        s.push_str(&format!("{rank}  "));
        for file in 0..9u8 {
            match board.piece_at(coord(file, rank)) {
                None => s.push_str(". "),
                Some(p) => {
                    s.push(piece_char(p));
                    s.push(' ');
                }
            }
        }
        s.pop();
        s.push('\n');
    }
    s
}

fn piece_char(p: Piece) -> char {
    let c = match p.kind {
        PieceKind::King => 'k',
        PieceKind::Advisor => 'a',
        PieceKind::Elephant => 'b',
        PieceKind::Horse => 'n',
        PieceKind::Rook => 'r',
        PieceKind::Cannon => 'c',
        PieceKind::Pawn => 'p',
    };
    if p.color == Color::Red {
        c.to_ascii_uppercase()
    } else {
        c
    }
}

fn piece_cn(p: Piece) -> &'static str {
    match p.kind {
        PieceKind::King => "帅/将",
        PieceKind::Advisor => "仕/士",
        PieceKind::Elephant => "相/象",
        PieceKind::Horse => "马",
        PieceKind::Rook => "车",
        PieceKind::Cannon => "炮",
        PieceKind::Pawn => "兵/卒",
    }
}

/// 着法注解（本地生成，零幻觉）：吃子（被吃子中文名）+ 将军标记
pub fn annotate_move(board: &Board, mv: Move) -> String {
    let mut parts = Vec::new();
    if let Some(cap) = board.piece_at(mv.to) {
        parts.push(format!("吃{}", piece_cn(cap)));
    }
    let child = board.make_move(mv);
    let opp = if board.side_to_move == Color::Red {
        Color::Black
    } else {
        Color::Red
    };
    if child.is_in_check(opp) {
        parts.push("将军".into());
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("({})", parts.join(","))
    }
}

// ---------------- 解析管线 ----------------

/// 归一化（第 3 层）：剥代码块围栏、删零宽字符/BOM、全角 ASCII 转半角、转小写
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '`' {
            continue; // 剥 ``` 围栏
        }
        if ('\u{200b}'..='\u{200f}').contains(&ch)
            || ch == '\u{feff}'
            || ch == '\u{2060}'
        {
            continue; // 零宽字符与 BOM
        }
        let c = if ('\u{ff01}'..='\u{ff5e}').contains(&ch) {
            // 全角 ASCII → 半角
            char::from_u32(ch as u32 - 0xfee0).unwrap_or(ch)
        } else {
            ch
        };
        out.push(c);
    }
    out.to_lowercase()
}

/// 坐标提取（第 4 层）：`[a-i] digit [可选分隔符] [a-i] digit`。
/// "着法:" 标记后第一个坐标对优先；否则取**最后一个**坐标对（模型总结论常在末尾）。
/// 分隔符可为空、`-` `–` `—` `~` `到` `至`。输出归一为 4 字符 UCCI（如 b2e2）。
pub fn extract_move_v2(s: &str) -> Option<String> {
    let s = normalize(s);
    let chars: Vec<char> = s.chars().collect();
    let mut found: Vec<(usize, usize)> = Vec::new(); // (file1 索引, file2 索引)

    let mut i = 0usize;
    while i + 1 < chars.len() {
        if is_file(chars[i]) && chars[i + 1].is_ascii_digit() {
            // 可选分隔符
            let mut j = i + 2;
            if j < chars.len() && is_sep(chars[j]) {
                j += 1; // 单个分隔符字符（- – — ~）
                // 中文"到/至"是两个字符，特殊处理
                if j < chars.len() && is_sep(chars[j]) {
                    j += 1;
                }
            }
            if j + 1 < chars.len() && is_file(chars[j]) && chars[j + 1].is_ascii_digit() {
                found.push((i, j));
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }

    if found.is_empty() {
        return None;
    }
    // "着法:" 标记优先：取标记之后第一个坐标对
    let marker = chars.iter().position(|c| *c == '着');
    let pick = if let Some(m) = marker {
        found
            .iter()
            .find(|(a, _)| *a >= m)
            .copied()
            .unwrap_or_else(|| *found.last().unwrap())
    } else {
        *found.last().unwrap()
    };
    let (a, b) = pick;
    Some(format!(
        "{}{}{}{}",
        chars[a],
        chars[a + 1],
        chars[b],
        chars[b + 1]
    ))
}

fn is_file(c: char) -> bool {
    matches!(c, 'a'..='i')
}

fn is_sep(c: char) -> bool {
    matches!(c, '-' | '–' | '—' | '~' | '到' | '至')
}

/// 旧版宽松提取（兼容测试）：首个 [a-i][0-9][a-i][0-9] 4 字符子串
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_v2_plain() {
        assert_eq!(extract_move_v2("着法: b2-e2"), Some("b2e2".to_string()));
    }

    #[test]
    fn extract_v2_no_sep() {
        assert_eq!(extract_move_v2("h2e2"), Some("h2e2".to_string()));
    }

    #[test]
    fn extract_v2_marker_priority() {
        assert_eq!(
            extract_move_v2("分析: 先出车。着法: h0-g2 后续 e3-e4"),
            Some("h0g2".to_string())
        );
    }

    #[test]
    fn extract_v2_last_fallback() {
        assert_eq!(extract_move_v2("可以考虑 e3-e4"), Some("e3e4".to_string()));
    }

    #[test]
    fn extract_v2_fullwidth() {
        assert_eq!(extract_move_v2("着法：ｂ２－ｅ２"), Some("b2e2".to_string()));
    }

    #[test]
    fn extract_v2_cjk_sep() {
        assert_eq!(extract_move_v2("着法: b2到e2"), Some("b2e2".to_string()));
    }

    #[test]
    fn extract_v2_fence() {
        assert_eq!(
            extract_move_v2("```\n着法: b2-e2\n```"),
            Some("b2e2".to_string())
        );
    }

    #[test]
    fn extract_plain_legacy() {
        assert_eq!(extract_ucci("h2e2"), Some("h2e2".to_string()));
    }

    #[test]
    fn extract_embedded_legacy() {
        assert_eq!(extract_ucci("abch2e2xyz"), Some("h2e2".to_string()));
    }

    #[test]
    fn ascii_board_shape() {
        let board = Board::parse_fen(
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
        )
        .unwrap();
        let s = ascii_board(&board);
        assert!(s.contains("0  R N B A K A B N R"));
        assert!(s.contains("9  r n b a k a b n r"));
        assert!(s.contains("a b c d e f g h i"));
    }
}

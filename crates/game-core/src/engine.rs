//! 统一引擎接口与内置引擎适配

use xiangqi_core::board::Board;
use xiangqi_core::types::Move;

use crate::model::EngineKind;

/// 引擎标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EngineId {
    /// 内置搜索引擎（纯 Rust，全平台）
    Builtin { depth: u8 },
    /// 大模型引擎（自配 key/端点，超时降级）
    Llm,
    /// 皮卡鱼（桌面端）
    Pikafish,
}

impl From<EngineKind> for EngineId {
    fn from(k: EngineKind) -> Self {
        match k {
            EngineKind::Builtin => EngineId::Builtin { depth: 4 },
            EngineKind::Llm => EngineId::Llm,
            EngineKind::Pikafish => EngineId::Pikafish,
        }
    }
}

/// 思考时间配置；0 = 不限（内置按深度）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineOptions {
    pub think_ms: u32,
}

/// 引擎走子结果
#[derive(Debug, Clone)]
pub struct EngineOutcome {
    pub mv: Move,
    pub source: EngineId,
    /// 降级原因（如 "大模型超时，已由内置引擎走子"）
    pub fallback_reason: Option<String>,
}

/// 引擎错误
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("轮走方无合法着（将死/困毙）")]
    NoLegalMove,
    #[error("引擎失败：{0}")]
    EngineFailed(String),
}

/// 统一引擎接口
pub trait Engine: Send {
    fn id(&self) -> EngineId;
    fn name(&self) -> String;
    fn best_move(
        &mut self,
        board: &Board,
        opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError>;

    /// 带对局历史的走子（LLM 提示词需要历史上下文）。默认忽略历史。
    fn best_move_with_history(
        &mut self,
        board: &Board,
        opts: EngineOptions,
        _history: &[Move],
    ) -> Result<EngineOutcome, EngineError> {
        self.best_move(board, opts)
    }
}

/// 内置引擎适配（包装 xiangqi-search）
#[derive(Debug, Clone)]
pub struct BuiltinEngineAdapter {
    depth: u8,
}

impl BuiltinEngineAdapter {
    pub fn new(depth: u8) -> Self {
        BuiltinEngineAdapter {
            depth: depth.max(1),
        }
    }
}

impl Engine for BuiltinEngineAdapter {
    fn id(&self) -> EngineId {
        EngineId::Builtin { depth: self.depth }
    }

    fn name(&self) -> String {
        format!("内置引擎·深度{}", self.depth)
    }

    fn best_move(
        &mut self,
        board: &Board,
        opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError> {
        let search = xiangqi_search::search::BuiltinEngine::new(self.depth);
        let report = if opts.think_ms > 0 {
            search.best_move_with_timeout(board, opts.think_ms as u64)
        } else {
            search.best_move(board)
        };
        match report.best_move {
            Some(mv) => Ok(EngineOutcome {
                mv,
                source: self.id(),
                fallback_reason: None,
            }),
            None => Err(EngineError::NoLegalMove),
        }
    }
}

/// 大模型引擎适配：LLM 提议 → 合法性已由客户端校验；失败/超时/非法着降级内置引擎
#[derive(Debug, Clone)]
pub struct LlmEngineAdapter {
    client: llm_engine::LlmClient,
    fallback_depth: u8,
}

impl LlmEngineAdapter {
    pub fn new(config: llm_engine::LlmConfig, fallback_depth: u8) -> Self {
        LlmEngineAdapter {
            client: llm_engine::LlmClient::new(config),
            fallback_depth: fallback_depth.max(1),
        }
    }

    /// 参谋搜索深度（Top-K / 单着评估）
    fn advisor_depth(&self) -> u8 {
        4
    }

    /// 模型失败/参谋兜底：内置引擎代走（永不走非法着）
    fn fallback_move(
        &self,
        board: &Board,
        opts: EngineOptions,
        reason: String,
    ) -> Result<EngineOutcome, EngineError> {
        let mut fb = BuiltinEngineAdapter::new(self.fallback_depth);
        let out = fb.best_move(board, opts)?;
        Ok(EngineOutcome {
            mv: out.mv,
            source: EngineId::Builtin {
                depth: self.fallback_depth,
            },
            fallback_reason: Some(reason),
        })
    }

    /// 组装 LlmRequestContext（全部来自本地规则/引擎，零幻觉）
    fn build_ctx(
        &self,
        board: &Board,
        history: &[Move],
        candidate_moves: Vec<(String, i32)>,
        veto: Option<String>,
    ) -> llm_engine::LlmRequestContext {
        let full_legal: Vec<String> = board.legal_moves().iter().map(|m| m.to_ucci()).collect();
        let history_ucci: Vec<String> = history
            .iter()
            .rev()
            .take(60)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| m.to_ucci())
            .collect();
        llm_engine::LlmRequestContext {
            ascii_board: Some(llm_engine::ascii_board(board)),
            history_ucci,
            candidate_moves,
            full_legal,
            veto_reason: veto,
            repetition_warning: repetition_warning(history),
        }
    }

    /// candidate：引擎 Top-K 短名单为决策池（模型只能从中选择）
    fn move_candidate(
        &self,
        board: &Board,
        opts: EngineOptions,
        history: &[Move],
    ) -> Result<EngineOutcome, EngineError> {
        let cfg = self.client.config();
        let k = cfg.top_k();
        let ms = opts.think_ms.max(800) as u64;
        let eng = xiangqi_search::BuiltinEngine::new(self.advisor_depth());
        let top = eng.top_moves(board, k, ms);
        if top.is_empty() {
            return self.fallback_move(board, opts, "引擎参谋未给出候选，已由内置引擎走子".into());
        }
        let candidate: Vec<(String, i32)> = top
            .iter()
            .map(|(m, sc)| (m.to_ucci(), *sc))
            .collect();
        let ctx = self.build_ctx(board, history, candidate, None);
        match self.client.propose_move_advised_sync(board, &ctx) {
            Ok(mv) => Ok(EngineOutcome {
                mv,
                source: EngineId::Llm,
                fallback_reason: None,
            }),
            Err(e) => self.fallback_move(board, opts, format!("大模型不可用（{e}），已由内置引擎走子")),
        }
    }

    /// gate：全量清单自由选 + 引擎护航否决（分差超阈值一票否决，带理由再问一次）
    fn move_gate(
        &self,
        board: &Board,
        opts: EngineOptions,
        history: &[Move],
    ) -> Result<EngineOutcome, EngineError> {
        let cfg = self.client.config();
        let ms = opts.think_ms.max(800) as u64;
        let eng = xiangqi_search::BuiltinEngine::new(self.advisor_depth());
        let top = eng.top_moves(board, 8, ms);
        let (best_ucci, best_score) = match top.first() {
            Some((m, sc)) => (m.to_ucci(), *sc),
            None => {
                return self.fallback_move(board, opts, "引擎参谋未完成评估，已由内置引擎走子".into())
            }
        };
        let thr = match cfg.veto_threshold() {
            Some(t) => t,
            None => {
                // blend=100：不否决，纯全量清单自由选
                let ctx = self.build_ctx(board, history, Vec::new(), None);
                return match self.client.propose_move_advised_sync(board, &ctx) {
                    Ok(mv) => Ok(EngineOutcome { mv, source: EngineId::Llm, fallback_reason: None }),
                    Err(e) => self.fallback_move(board, opts, format!("大模型不可用（{e}），已由内置引擎走子")),
                };
            }
        };

        let ctx0 = self.build_ctx(board, history, Vec::new(), None);
        let first = self.client.propose_move_advised_sync(board, &ctx0);
        let Ok(mv) = first else {
            let e = first.unwrap_err();
            return self.fallback_move(board, opts, format!("大模型不可用（{e}），已由内置引擎走子"));
        };
        let d = self.advisor_depth().saturating_sub(1).max(1);
        let mv_score = eng.evaluate_move(board, mv, d);
        let loss = best_score - mv_score;
        if loss <= thr {
            return Ok(EngineOutcome { mv, source: EngineId::Llm, fallback_reason: None });
        }
        // 参谋否决：带理由再问一次（对第二次选择重新评估，不信第二次的回答）
        let veto = format!(
            "你上一次选择的 {} 会被引擎惩罚（{}，相对最佳损失 {} 厘兵）。请重新从候选清单中选择，优先考虑「最佳/均势」档；先「分析:」一句，最后一行「着法: 起点-终点」。",
            mv.to_ucci(),
            llm_engine::bucket_cp(loss),
            loss
        );
        let ctx1 = self.build_ctx(board, history, Vec::new(), Some(veto));
        match self.client.propose_move_advised_sync(board, &ctx1) {
            Ok(mv2) => {
                let mv2_score = eng.evaluate_move(board, mv2, d);
                let loss2 = best_score - mv2_score;
                if loss2 <= thr {
                    Ok(EngineOutcome {
                        mv: mv2,
                        source: EngineId::Llm,
                        fallback_reason: Some(format!(
                            "参谋否决：原选 {}(损失 {} 厘兵)，模型改选 {}",
                            mv.to_ucci(),
                            loss,
                            mv2.to_ucci()
                        )),
                    })
                } else {
                    self.gate_veto_fallback(board, opts, &best_ucci, loss, mv)
                }
            }
            Err(_e) => self.gate_veto_fallback(board, opts, &best_ucci, loss, mv),
        }
    }

    /// gate 否决兜底：采用引擎最佳着法（参谋职责，非模型失败降级）
    fn gate_veto_fallback(
        &self,
        _board: &Board,
        _opts: EngineOptions,
        best_ucci: &str,
        loss: i32,
        vetoed: Move,
    ) -> Result<EngineOutcome, EngineError> {
        let bm = Move::from_ucci(best_ucci)
            .ok_or_else(|| EngineError::EngineFailed("参谋最佳着解析失败".into()))?;
        Ok(EngineOutcome {
            mv: bm,
            source: EngineId::Builtin {
                depth: self.fallback_depth,
            },
            fallback_reason: Some(format!(
                "参谋否决 {}(损失 {} 厘兵)，采用引擎最佳着法 {}",
                vetoed.to_ucci(),
                loss,
                best_ucci
            )),
        })
    }
}

impl Engine for LlmEngineAdapter {
    fn id(&self) -> EngineId {
        EngineId::Llm
    }
    fn name(&self) -> String {
        "大模型引擎".to_string()
    }
    fn best_move(
        &mut self,
        board: &Board,
        opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError> {
        self.best_move_with_history(board, opts, &[])
    }
    fn best_move_with_history(
        &mut self,
        board: &Board,
        opts: EngineOptions,
        history: &[Move],
    ) -> Result<EngineOutcome, EngineError> {
        if board.legal_moves().is_empty() {
            return Err(EngineError::NoLegalMove);
        }
        match self.client.config().advisor.as_str() {
            "gate" => self.move_gate(board, opts, history),
            "candidate" => self.move_candidate(board, opts, history),
            _ => {
                // off：全量合法清单（Prompt v2 基线）
                let ctx = self.build_ctx(board, history, Vec::new(), None);
                match self.client.propose_move_advised_sync(board, &ctx) {
                    Ok(mv) => Ok(EngineOutcome { mv, source: EngineId::Llm, fallback_reason: None }),
                    Err(e) => self.fallback_move(board, opts, format!("大模型不可用（{e}），已由内置引擎走子")),
                }
            }
        }
    }
}

/// 最近着法来回重复警示（供提示词使用；仅提示，不做规则判定）
fn repetition_warning(history: &[Move]) -> Option<String> {
    let n = history.len();
    if n >= 4 {
        let l = &history[n - 4..];
        let rev = |a: Move, b: Move| a.from == b.to && a.to == b.from;
        if rev(l[0], l[2]) && rev(l[1], l[3]) {
            return Some(
                "最近着法出现来回重复。长将/长捉判负，重复局面会被视为无效——请选择打破循环的着法。"
                    .into(),
            );
        }
    }
    None
}

/// 大模型未配置时的占位适配器（提示配置）
#[derive(Debug, Clone)]
pub struct LlmUnconfiguredAdapter;

impl Engine for LlmUnconfiguredAdapter {
    fn id(&self) -> EngineId {
        EngineId::Llm
    }
    fn name(&self) -> String {
        "大模型引擎（未配置）".to_string()
    }
    fn best_move(
        &mut self,
        _board: &Board,
        _opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError> {
        Err(EngineError::EngineFailed(
            "未配置大模型，请在设置中填写 base_url / api_key / model".into(),
        ))
    }
}

/// 皮卡鱼适配（M7 实现；当前返回未接入错误）
#[derive(Debug, Clone)]
pub struct PikafishEngineAdapter;

impl Engine for PikafishEngineAdapter {
    fn id(&self) -> EngineId {
        EngineId::Pikafish
    }
    fn name(&self) -> String {
        "皮卡鱼引擎".to_string()
    }
    fn best_move(
        &mut self,
        _board: &Board,
        _opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError> {
        Err(EngineError::EngineFailed("皮卡鱼尚未接入（M7）".into()))
    }
}

/// 引擎管理：按 EngineId 创建实例（持有大模型配置）
#[derive(Debug, Clone, Default)]
pub struct EngineManager {
    llm: Option<llm_engine::LlmConfig>,
}

impl EngineManager {
    pub fn new(llm: Option<llm_engine::LlmConfig>) -> Self {
        EngineManager { llm }
    }

    pub fn set_llm(&mut self, config: Option<llm_engine::LlmConfig>) {
        self.llm = config;
    }

    pub fn llm_config(&self) -> Option<&llm_engine::LlmConfig> {
        self.llm.as_ref()
    }

    pub fn create(&self, id: EngineId) -> Box<dyn Engine> {
        match id {
            EngineId::Builtin { depth } => Box::new(BuiltinEngineAdapter::new(depth)),
            EngineId::Llm => match &self.llm {
                Some(cfg) => Box::new(LlmEngineAdapter::new(cfg.clone(), 4)),
                None => Box::new(LlmUnconfiguredAdapter),
            },
            EngineId::Pikafish => Box::new(PikafishEngineAdapter),
        }
    }
}

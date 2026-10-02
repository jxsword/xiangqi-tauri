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

/// 大模型引擎适配（M6 实现；当前返回未接入错误）
#[derive(Debug, Clone)]
pub struct LlmEngineAdapter;

impl Engine for LlmEngineAdapter {
    fn id(&self) -> EngineId {
        EngineId::Llm
    }
    fn name(&self) -> String {
        "大模型引擎".to_string()
    }
    fn best_move(
        &mut self,
        _board: &Board,
        _opts: EngineOptions,
    ) -> Result<EngineOutcome, EngineError> {
        Err(EngineError::EngineFailed("大模型引擎尚未接入（M6）".into()))
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

/// 引擎管理：按 EngineId 创建实例
#[derive(Debug, Clone, Default)]
pub struct EngineManager;

impl EngineManager {
    pub fn create(&self, id: EngineId) -> Box<dyn Engine> {
        match id {
            EngineId::Builtin { depth } => Box::new(BuiltinEngineAdapter::new(depth)),
            EngineId::Llm => Box::new(LlmEngineAdapter),
            EngineId::Pikafish => Box::new(PikafishEngineAdapter),
        }
    }
}

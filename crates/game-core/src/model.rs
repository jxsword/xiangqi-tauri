//! 对局共享模型：模式、引擎类型、结果

use serde::{Deserialize, Serialize};

use xiangqi_core::types::Color;

/// 对局模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    /// 人机对战（人对人也可用 HumanVsHuman，阶段一聚焦人机/机器）
    HumanVsMachine,
    /// 机器对战（引擎对引擎）
    MachineVsMachine,
    /// 双人对弈（本地轮流）
    HumanVsHuman,
}

/// 引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineKind {
    /// 内置搜索引擎（纯 Rust，全平台可用，LLM 降级目标）
    Builtin,
    /// 大模型引擎（自配 key/端点，10s 超时降级）
    Llm,
    /// 皮卡鱼（NNUE+UCI，桌面端；Android 不可用）
    Pikafish,
}

/// 引擎配置
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineConfig {
    pub kind: EngineKind,
    /// Builtin 的搜索深度（1..=6 难度档）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
}

impl EngineConfig {
    pub fn builtin(depth: u8) -> Self {
        EngineConfig {
            kind: EngineKind::Builtin,
            depth: Some(depth),
        }
    }

    pub fn llm() -> Self {
        EngineConfig {
            kind: EngineKind::Llm,
            depth: None,
        }
    }

    pub fn pikafish() -> Self {
        EngineConfig {
            kind: EngineKind::Pikafish,
            depth: None,
        }
    }
}

/// 对局结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameResult {
    /// 获胜方
    Win(Color),
    /// 和棋
    Draw,
    /// 对局被中止（前端「停止」按钮）
    Aborted,
}

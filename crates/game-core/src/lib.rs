//! game-core: 对局状态机、统一引擎接口、机器对战调度
//!
//! 本模块分阶段落地：M4 先提供共享模型（model），M5 提供 Game 状态机与 EngineTrait。

pub mod model;

pub use model::{EngineConfig, EngineKind, GameMode, GameResult};

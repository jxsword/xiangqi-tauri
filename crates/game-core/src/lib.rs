//! game-core: 对局状态机、统一引擎接口、机器对战调度

pub mod engine;
pub mod game;
pub mod model;

pub use engine::{
    BuiltinEngineAdapter, Engine, EngineError, EngineId, EngineManager, EngineOptions,
    EngineOutcome,
};
pub use game::{Game, GameError, GameEvent};
pub use model::{EngineConfig, EngineKind, GameMode, GameResult};

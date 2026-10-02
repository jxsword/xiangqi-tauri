//! xiangqi-search: 内置搜索引擎（迭代加深 α-β + 置换表 + 局面评估）
//!
//! 供 game-core 的 BuiltinEngine 使用（人机对战、LLM 降级、Android 端内置引擎）。

pub mod eval;
pub mod search;

pub use eval::{piece_value, Evaluator, MaterialPositionEvaluator, MATE};
pub use search::{BuiltinEngine, SearchConfig, SearchReport};

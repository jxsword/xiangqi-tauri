//! xiangqi-core: 中国象棋规则核心（局面表示、走法生成、胜负判定）
//!
//! 纯逻辑、无 I/O、无异步，便于单元测试与未来 WASM 化。

pub mod board;
pub mod types;

pub use board::Board;
pub use types::*;

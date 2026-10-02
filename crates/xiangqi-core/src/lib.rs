//! xiangqi-core: 中国象棋规则核心（局面表示、走法生成、胜负判定）
//!
//! 纯逻辑、无 I/O、无异步，便于单元测试与未来 WASM 化。

pub const BOARD_SIZE: usize = 90; // 9 列 × 10 行 = 90 个交叉点

pub fn placeholder() -> &'static str {
    "xiangqi-core skeleton"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_ok() {
        assert_eq!(BOARD_SIZE, 90);
        assert!(placeholder().contains("xiangqi-core"));
    }
}

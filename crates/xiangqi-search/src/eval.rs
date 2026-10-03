//! 评估函数：子力 + 位置表（PST），红方正数，中分=0
//!
//! 位置表为红方视角（行索引 = rank 0..9，rank0=红方底线）；黑方取 rank 镜像。

use xiangqi_core::board::Board;
use xiangqi_core::types::{sq_to_file_rank, Color, PieceKind};

/// 将死/困毙得分基准（须大于一切子力估值）
pub const MATE: i32 = 1_000_000;

/// 子力基础值（红方正数）
pub fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::King => 100_000,
        PieceKind::Rook => 600,
        PieceKind::Horse => 320,
        PieceKind::Cannon => 320,
        PieceKind::Advisor => 120,
        PieceKind::Elephant => 120,
        PieceKind::Pawn => 60,
    }
}

// 经典中国象棋位置表（红方视角；仅存 file 0..4，按列对称展开）。
// 行 = rank：0=红方底线 … 9=黑方底线。

const HORSE_PST: [[i32; 5]; 10] = [
    [4, 8, 16, 12, 4],
    [4, 10, 28, 16, 8],
    [12, 14, 16, 18, 10],
    [14, 18, 22, 20, 12],
    [18, 20, 24, 22, 14],
    [18, 20, 22, 20, 12],
    [14, 18, 22, 20, 12],
    [12, 14, 16, 18, 10],
    [4, 10, 28, 16, 8],
    [4, 8, 16, 12, 4],
];

const CANNON_PST: [[i32; 5]; 10] = [
    [6, 4, 0, 10, 0],
    [4, 2, 10, 6, 0],
    [2, 8, 12, 8, 2],
    [4, 8, 10, 6, 4],
    [6, 10, 12, 10, 6],
    [6, 10, 12, 10, 6],
    [4, 8, 10, 6, 4],
    [2, 8, 12, 8, 2],
    [4, 2, 10, 6, 0],
    [6, 4, 0, 10, 0],
];

const ROOK_PST: [[i32; 5]; 10] = [
    [14, 14, 12, 18, 16],
    [16, 20, 18, 24, 20],
    [12, 12, 12, 18, 18],
    [10, 14, 12, 12, 10],
    [8, 12, 16, 12, 8],
    [8, 12, 16, 12, 8],
    [10, 14, 12, 12, 10],
    [12, 12, 12, 18, 18],
    [16, 20, 18, 24, 20],
    [14, 14, 12, 18, 16],
];

/// 兵卒位置表：rank5+ 已过河（红方视角），越深入敌方得分越高
const PAWN_PST: [[i32; 5]; 10] = [
    [0, 3, 6, 9, 12],
    [18, 36, 56, 80, 120],
    [14, 26, 42, 60, 80],
    [10, 20, 30, 38, 50],
    [6, 12, 18, 24, 30],
    [2, 6, 10, 14, 18],
    [0, 2, 4, 6, 8],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
];

/// 红方视角位置分（file 对称展开）
fn pst(kind: PieceKind, file: u8, rank: u8) -> i32 {
    let f = (file.min(8 - file)) as usize; // 0..=4
    let r = rank as usize;
    match kind {
        PieceKind::Horse => HORSE_PST[r][f],
        PieceKind::Cannon => CANNON_PST[r][f],
        PieceKind::Rook => ROOK_PST[r][f],
        PieceKind::Pawn => PAWN_PST[r][f],
        _ => 0,
    }
}

/// 评估器扩展点（未来可替换为 NNUE 等）
pub trait Evaluator {
    fn evaluate(&self, board: &Board) -> i32;
}

/// 子力 + 位置评估（红方正数）
pub struct MaterialPositionEvaluator;

impl Evaluator for MaterialPositionEvaluator {
    fn evaluate(&self, board: &Board) -> i32 {
        let mut score = 0i32;
        for sq in 0..90u8 {
            if let Some(p) = board.piece_at(sq) {
                let (file, rank) = sq_to_file_rank(sq);
                let base = piece_value(p.kind);
                let pos = match p.color {
                    Color::Red => pst(p.kind, file, rank),
                    Color::Black => pst(p.kind, file, 9 - rank),
                };
                if p.color == Color::Red {
                    score += base + pos;
                } else {
                    score -= base + pos;
                }
            }
        }
        score
    }
}

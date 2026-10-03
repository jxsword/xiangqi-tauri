//! 基本类型：颜色、兵种、棋子、走法、游戏状态

use serde::{Deserialize, Serialize};

/// 棋盘交叉点总数：9 列 × 10 行 = 90
pub const BOARD_SIZE: usize = 90;

/// 棋子颜色。红方先行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Color {
    Red,
    Black,
}

impl Color {
    pub fn opposite(self) -> Color {
        match self {
            Color::Red => Color::Black,
            Color::Black => Color::Red,
        }
    }
}

/// 兵种
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PieceKind {
    King,     // 帅/将
    Advisor,  // 仕/士
    Elephant, // 相/象
    Horse,    // 马
    Rook,     // 车
    Cannon,   // 炮
    Pawn,     // 兵/卒
}

/// 一枚棋子
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Piece {
    pub kind: PieceKind,
    pub color: Color,
}

/// 一步走法：from/to 均为 0..90 交叉点索引（sq = rank*9 + file）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    pub from: u8,
    pub to: u8,
}

impl Move {
    pub fn new(from: u8, to: u8) -> Self {
        Move { from, to }
    }

    /// UCCI 4 字符记法，如 h2e2（file a-i, rank 0-9，rank0=红方底线）
    pub fn to_ucci(&self) -> String {
        let (f1, r1) = sq_to_file_rank(self.from);
        let (f2, r2) = sq_to_file_rank(self.to);
        format!("{}{}{}{}", file_to_char(f1), r1, file_to_char(f2), r2)
    }

    pub fn from_ucci(s: &str) -> Option<Move> {
        let b = s.as_bytes();
        if b.len() != 4 {
            return None;
        }
        let f1 = char_to_file(b[0] as char)?;
        let r1 = (b[1] as char).to_digit(10)?;
        let f2 = char_to_file(b[2] as char)?;
        let r2 = (b[3] as char).to_digit(10)?;
        if r1 > 9 || r2 > 9 {
            return None;
        }
        Some(Move::new(coord(f1, r1 as u8), coord(f2, r2 as u8)))
    }
}

/// 对局状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameStatus {
    /// 正常进行
    Ongoing,
    /// 当前轮到的一方被将军
    Check,
    /// 将死：winner 胜
    Checkmate(Color),
    /// 困毙：无着可走的一方负（中国象棋规则：走子方负）
    Stalemate(Color),
}

/// file: 0..8（a..i），rank: 0..9（0=红方底线）
#[inline]
pub fn coord(file: u8, rank: u8) -> u8 {
    rank * 9 + file
}

#[inline]
pub fn sq_to_file_rank(sq: u8) -> (u8, u8) {
    (sq % 9, sq / 9)
}

#[inline]
pub fn file_to_char(file: u8) -> char {
    (b'a' + file) as char
}

#[inline]
pub fn char_to_file(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    if ('a'..='i').contains(&c) {
        Some(c as u8 - b'a')
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ucci_roundtrip() {
        let mv = Move::new(coord(7, 2), coord(4, 2)); // h2e2（炮二平五）
        assert_eq!(mv.to_ucci(), "h2e2");
        assert_eq!(Move::from_ucci("h2e2"), Some(mv));
        assert_eq!(Move::from_ucci("zzzz"), None);
        assert_eq!(Move::from_ucci("h2e"), None);
        assert_eq!(Move::from_ucci("h2e20"), None);
    }

    #[test]
    fn coordinate_helpers() {
        assert_eq!(coord(0, 0), 0);
        assert_eq!(coord(8, 9), 89);
        assert_eq!(sq_to_file_rank(25), (7, 2));
        assert_eq!(file_to_char(0), 'a');
        assert_eq!(char_to_file('I'), Some(8));
        assert_eq!(char_to_file('j'), None);
    }
}

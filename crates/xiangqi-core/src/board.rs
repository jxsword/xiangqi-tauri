//! 棋盘：局面表示、FEN、走法生成、将军/胜负判定
//!
//! 坐标约定：sq = rank*9 + file；file 0..8 = a..i（红方视角从左到右）；
//! rank 0 = 红方底线（下方），rank 9 = 黑方底线（上方）。

use crate::types::*;

/// 棋盘
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub pieces: [Option<Piece>; BOARD_SIZE],
    pub side_to_move: Color,
}

impl Default for Board {
    fn default() -> Self {
        Self::start_position()
    }
}

impl Board {
    /// 标准初始局面
    pub fn start_position() -> Board {
        let mut b = Board {
            pieces: [None; BOARD_SIZE],
            side_to_move: Color::Red,
        };
        let back_rank: [(u8, PieceKind); 9] = [
            (0, PieceKind::Rook),
            (1, PieceKind::Horse),
            (2, PieceKind::Elephant),
            (3, PieceKind::Advisor),
            (4, PieceKind::King),
            (5, PieceKind::Advisor),
            (6, PieceKind::Elephant),
            (7, PieceKind::Horse),
            (8, PieceKind::Rook),
        ];
        for (file, kind) in back_rank {
            b.set(
                coord(file, 0),
                Some(Piece {
                    kind,
                    color: Color::Red,
                }),
            );
            b.set(
                coord(file, 9),
                Some(Piece {
                    kind,
                    color: Color::Black,
                }),
            );
        }
        b.set(
            coord(1, 2),
            Some(Piece {
                kind: PieceKind::Cannon,
                color: Color::Red,
            }),
        ); // b2 炮
        b.set(
            coord(7, 2),
            Some(Piece {
                kind: PieceKind::Cannon,
                color: Color::Red,
            }),
        ); // h2 炮
        b.set(
            coord(1, 7),
            Some(Piece {
                kind: PieceKind::Cannon,
                color: Color::Black,
            }),
        ); // b7 炮
        b.set(
            coord(7, 7),
            Some(Piece {
                kind: PieceKind::Cannon,
                color: Color::Black,
            }),
        ); // h7 炮
        for file in [0, 2, 4, 6, 8] {
            b.set(
                coord(file, 3),
                Some(Piece {
                    kind: PieceKind::Pawn,
                    color: Color::Red,
                }),
            );
            b.set(
                coord(file, 6),
                Some(Piece {
                    kind: PieceKind::Pawn,
                    color: Color::Black,
                }),
            );
        }
        b
    }

    pub fn set(&mut self, sq: u8, piece: Option<Piece>) {
        self.pieces[sq as usize] = piece;
    }

    pub fn piece_at(&self, sq: u8) -> Option<Piece> {
        self.pieces[sq as usize]
    }

    pub fn find_king(&self, color: Color) -> Option<u8> {
        for (i, p) in self.pieces.iter().enumerate() {
            if let Some(p) = p {
                if p.kind == PieceKind::King && p.color == color {
                    return Some(i as u8);
                }
            }
        }
        None
    }

    // ---------------- FEN ----------------

    /// 解析中国象棋 FEN（含轮走方，忽略 - - 0 1 后段）
    pub fn parse_fen(fen: &str) -> Result<Board, String> {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.is_empty() || parts[0].is_empty() {
            return Err("空 FEN".into());
        }
        let rows: Vec<&str> = parts[0].split('/').collect();
        if rows.len() != 10 {
            return Err(format!("FEN 应为 10 行，实际 {}", rows.len()));
        }
        let mut board = Board {
            pieces: [None; BOARD_SIZE],
            side_to_move: Color::Red,
        };
        // FEN 从上到下 = rank 9 → 0
        for (i, row) in rows.iter().enumerate() {
            let rank = 9 - i as u8;
            let mut file: u8 = 0;
            for c in row.chars() {
                if let Some(d) = c.to_digit(10) {
                    file += d as u8;
                } else {
                    if file > 8 {
                        return Err(format!("行 {} 溢出：{}", row, file));
                    }
                    let (kind, color) =
                        fen_char_to_piece(c).ok_or_else(|| format!("无法识别的棋子字符：{c}"))?;
                    board.set(coord(file, rank), Some(Piece { kind, color }));
                    file += 1;
                }
            }
            if file != 9 {
                return Err(format!("行 {} 的列数 = {} ≠ 9", row, file));
            }
        }
        if let Some(turn) = parts.get(1) {
            board.side_to_move = match *turn {
                "w" => Color::Red,
                "b" => Color::Black,
                _ => return Err(format!("非法轮走方：{turn}")),
            };
        }
        Ok(board)
    }

    pub fn to_fen(&self) -> String {
        let mut fen = String::new();
        for rank in (0..10).rev() {
            let mut empty = 0u8;
            for file in 0..9 {
                match self.piece_at(coord(file, rank)) {
                    None => empty += 1,
                    Some(p) => {
                        if empty > 0 {
                            fen.push(char::from_digit(empty as u32, 10).unwrap());
                            empty = 0;
                        }
                        fen.push(piece_to_fen_char(p));
                    }
                }
            }
            if empty > 0 {
                fen.push(char::from_digit(empty as u32, 10).unwrap());
            }
            if rank != 0 {
                fen.push('/');
            }
        }
        fen.push(' ');
        fen.push(if self.side_to_move == Color::Red {
            'w'
        } else {
            'b'
        });
        fen.push_str(" - - 0 1");
        fen
    }

    // ---------------- 走法生成 ----------------

    /// 伪合法着法（可能包含走后送将的着法）
    pub fn pseudo_legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::new();
        for from in 0..BOARD_SIZE as u8 {
            if let Some(p) = self.piece_at(from) {
                if p.color != self.side_to_move {
                    continue;
                }
                self.gen_piece_moves(from, p, &mut moves);
            }
        }
        moves
    }

    /// 合法着法（过滤走后己方被将军 / 将帅互见）
    pub fn legal_moves(&self) -> Vec<Move> {
        self.pseudo_legal_moves()
            .into_iter()
            .filter(|m| !self.make_move(*m).is_in_check(self.side_to_move))
            .collect()
    }

    /// 应用走法，返回新棋盘（不可变风格）
    pub fn make_move(&self, mv: Move) -> Board {
        let mut next = self.clone();
        let piece = next.piece_at(mv.from).expect("make_move: from 无子");
        next.set(mv.from, None);
        next.set(mv.to, Some(piece));
        next.side_to_move = self.side_to_move.opposite();
        next
    }

    fn gen_piece_moves(&self, from: u8, p: Piece, out: &mut Vec<Move>) {
        let (fx, fy) = sq_to_file_rank(from);
        match p.kind {
            PieceKind::King => {
                for (dx, dy) in [(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
                    let to = try_sq(fx, fy, dx, dy);
                    if let Some(to) = to {
                        if in_palace(to, p.color) && self.target_ok(to, p.color) {
                            out.push(Move::new(from, to));
                        }
                    }
                }
            }
            PieceKind::Advisor => {
                for (dx, dy) in [(1i8, 1i8), (1, -1), (-1, 1), (-1, -1)] {
                    let to = try_sq(fx, fy, dx, dy);
                    if let Some(to) = to {
                        if in_palace(to, p.color) && self.target_ok(to, p.color) {
                            out.push(Move::new(from, to));
                        }
                    }
                }
            }
            PieceKind::Elephant => {
                for (dx, dy) in [(2i8, 2i8), (2, -2), (-2, 2), (-2, -2)] {
                    let to = try_sq(fx, fy, dx, dy);
                    if let Some(to) = to {
                        let (mx, my) = (fx as i8 + dx / 2, fy as i8 + dy / 2);
                        let mid = coord(mx as u8, my as u8);
                        // 塞象眼 + 不过河 + 目标可落
                        if self.piece_at(mid).is_none()
                            && !crossed_river(to, p.color)
                            && self.target_ok(to, p.color)
                        {
                            out.push(Move::new(from, to));
                        }
                    }
                }
            }
            PieceKind::Horse => {
                for (dx, dy) in [
                    (1i8, 2i8),
                    (1, -2),
                    (-1, 2),
                    (-1, -2),
                    (2, 1i8),
                    (2, -1),
                    (-2, 1),
                    (-2, -1),
                ] {
                    let to = try_sq(fx, fy, dx, dy);
                    if let Some(to) = to {
                        // 马腿：沿主位移方向"一步"（符号方向）
                        let (legx, legy) = if dx.abs() == 1 {
                            (0i8, dy.signum())
                        } else {
                            (dx.signum(), 0i8)
                        };
                        let leg = coord((fx as i8 + legx) as u8, (fy as i8 + legy) as u8);
                        if self.piece_at(leg).is_none() && self.target_ok(to, p.color) {
                            out.push(Move::new(from, to));
                        }
                    }
                }
            }
            PieceKind::Rook => {
                for (dx, dy) in [(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
                    let mut x = fx as i8 + dx;
                    let mut y = fy as i8 + dy;
                    while in_board(x, y) {
                        let to = coord(x as u8, y as u8);
                        match self.piece_at(to) {
                            None => out.push(Move::new(from, to)),
                            Some(occ) => {
                                if occ.color != p.color {
                                    out.push(Move::new(from, to));
                                }
                                break;
                            }
                        }
                        x += dx;
                        y += dy;
                    }
                }
            }
            PieceKind::Cannon => {
                for (dx, dy) in [(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
                    let mut x = fx as i8 + dx;
                    let mut y = fy as i8 + dy;
                    let mut screen = false;
                    while in_board(x, y) {
                        let to = coord(x as u8, y as u8);
                        match self.piece_at(to) {
                            None => {
                                if !screen {
                                    out.push(Move::new(from, to));
                                }
                            }
                            Some(occ) => {
                                if !screen {
                                    screen = true; // 找到炮架
                                } else if occ.color != p.color {
                                    out.push(Move::new(from, to)); // 隔一吃
                                    break;
                                } else {
                                    break;
                                }
                            }
                        }
                        x += dx;
                        y += dy;
                    }
                }
            }
            PieceKind::Pawn => {
                let fwd: i8 = if p.color == Color::Red { 1 } else { -1 };
                if let Some(to) = try_sq(fx, fy, 0, fwd) {
                    if self.target_ok(to, p.color) {
                        out.push(Move::new(from, to));
                    }
                }
                if crossed_river(from, p.color) {
                    for dx in [-1i8, 1i8] {
                        if let Some(to) = try_sq(fx, fy, dx, 0) {
                            if self.target_ok(to, p.color) {
                                out.push(Move::new(from, to));
                            }
                        }
                    }
                }
            }
        }
    }

    fn target_ok(&self, to: u8, color: Color) -> bool {
        match self.piece_at(to) {
            None => true,
            Some(occ) => occ.color != color,
        }
    }

    // ---------------- 攻击与状态 ----------------

    /// 判断 sq 是否被 by_color 攻击
    pub fn is_square_attacked(&self, sq: u8, by_color: Color) -> bool {
        let (fx, fy) = sq_to_file_rank(sq);

        // 马：8 个攻击源
        for (dx, dy) in [
            (1i8, 2i8),
            (1, -2),
            (-1, 2),
            (-1, -2),
            (2, 1i8),
            (2, -1),
            (-2, 1),
            (-2, -1),
        ] {
            let sx = fx as i8 - dx;
            let sy = fy as i8 - dy;
            if !in_board(sx, sy) {
                continue;
            }
            let src = coord(sx as u8, sy as u8);
            if let Some(p) = self.piece_at(src) {
                if p.color == by_color && p.kind == PieceKind::Horse {
                    // 马腿：候选源沿主方向一步（源 -> 目标 sq 的方向为 (dx, dy)）
                    let (legx, legy) = if dx.abs() == 1 {
                        (0i8, dy.signum())
                    } else {
                        (dx.signum(), 0i8)
                    };
                    let leg = coord((sx + legx) as u8, (sy + legy) as u8);
                    if self.piece_at(leg).is_none() {
                        return true;
                    }
                }
            }
        }

        // 兵：直攻与横攻
        for (dx, dy) in [
            (0i8, 1i8),
            (0, -1),
            (1, 0),
            (-1, 0),
            (1, 1),
            (-1, 1),
            (1, -1),
            (-1, -1),
        ] {
            let sx = fx as i8 - dx;
            let sy = fy as i8 - dy;
            if !in_board(sx, sy) {
                continue;
            }
            let src = coord(sx as u8, sy as u8);
            if let Some(p) = self.piece_at(src) {
                if p.color == by_color && p.kind == PieceKind::Pawn {
                    let fwd: i8 = if by_color == Color::Red { 1 } else { -1 };
                    let straight = dx == 0 && dy == fwd;
                    let lateral = dy == 0
                        && (by_color == Color::Red && src / 9 >= 5
                            || by_color == Color::Black && src / 9 <= 4);
                    if straight || lateral {
                        return true;
                    }
                }
            }
        }

        // 车/炮/帅：四方向扫描
        for (dx, dy) in [(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
            let mut x = fx as i8 + dx;
            let mut y = fy as i8 + dy;
            let mut first: Option<Piece> = None;
            while in_board(x, y) {
                let at = coord(x as u8, y as u8);
                if let Some(occ) = self.piece_at(at) {
                    match first {
                        None => {
                            // 第一个阻挡：己方车/帅直线攻击；其余作为炮架继续扫描
                            if occ.color == by_color
                                && (occ.kind == PieceKind::Rook || occ.kind == PieceKind::King)
                            {
                                return true;
                            }
                            first = Some(occ);
                        }
                        Some(_) => {
                            // 第二个阻挡：己方炮隔架攻击；再往后不再延伸
                            if occ.color == by_color && occ.kind == PieceKind::Cannon {
                                return true;
                            }
                            break;
                        }
                    }
                }
                x += dx;
                y += dy;
            }
        }
        false
    }

    /// 指定颜色是否被将军（含将帅互见）
    pub fn is_in_check(&self, color: Color) -> bool {
        match self.find_king(color) {
            Some(king) => self.is_square_attacked(king, color.opposite()),
            None => false, // 无帅（异常局面）视作未将军
        }
    }

    /// 当前对局状态
    pub fn game_status(&self) -> GameStatus {
        let legal = self.legal_moves();
        if legal.is_empty() {
            if self.is_in_check(self.side_to_move) {
                GameStatus::Checkmate(self.side_to_move.opposite())
            } else {
                GameStatus::Stalemate(self.side_to_move) // 困毙，走子方负
            }
        } else if self.is_in_check(self.side_to_move) {
            GameStatus::Check
        } else {
            GameStatus::Ongoing
        }
    }
}

// ---------------- 工具函数 ----------------

#[inline]
fn in_board(x: i8, y: i8) -> bool {
    (0..9).contains(&x) && (0..10).contains(&y)
}

#[inline]
fn try_sq(fx: u8, fy: u8, dx: i8, dy: i8) -> Option<u8> {
    let x = fx as i8 + dx;
    let y = fy as i8 + dy;
    if in_board(x, y) {
        Some(coord(x as u8, y as u8))
    } else {
        None
    }
}

/// 是否在九宫内
#[inline]
fn in_palace(sq: u8, color: Color) -> bool {
    let (f, r) = sq_to_file_rank(sq);
    if !(3..=5).contains(&f) {
        return false;
    }
    match color {
        Color::Red => (0..=2).contains(&r),
        Color::Black => (7..=9).contains(&r),
    }
}

/// 是否越过河界（红方过河到黑方半场，黑方反之）
#[inline]
fn crossed_river(sq: u8, color: Color) -> bool {
    let r = sq / 9;
    match color {
        Color::Red => r >= 5,
        Color::Black => r <= 4,
    }
}

/// FEN 字符 ↔ 棋子
fn fen_char_to_piece(c: char) -> Option<(PieceKind, Color)> {
    let (kind, color) = match c {
        'K' => (PieceKind::King, Color::Red),
        'A' => (PieceKind::Advisor, Color::Red),
        'B' => (PieceKind::Elephant, Color::Red),
        'N' => (PieceKind::Horse, Color::Red),
        'R' => (PieceKind::Rook, Color::Red),
        'C' => (PieceKind::Cannon, Color::Red),
        'P' => (PieceKind::Pawn, Color::Red),
        'k' => (PieceKind::King, Color::Black),
        'a' => (PieceKind::Advisor, Color::Black),
        'b' => (PieceKind::Elephant, Color::Black),
        'n' => (PieceKind::Horse, Color::Black),
        'r' => (PieceKind::Rook, Color::Black),
        'c' => (PieceKind::Cannon, Color::Black),
        'p' => (PieceKind::Pawn, Color::Black),
        _ => return None,
    };
    Some((kind, color))
}

fn piece_to_fen_char(p: Piece) -> char {
    match (p.kind, p.color) {
        (PieceKind::King, Color::Red) => 'K',
        (PieceKind::Advisor, Color::Red) => 'A',
        (PieceKind::Elephant, Color::Red) => 'B',
        (PieceKind::Horse, Color::Red) => 'N',
        (PieceKind::Rook, Color::Red) => 'R',
        (PieceKind::Cannon, Color::Red) => 'C',
        (PieceKind::Pawn, Color::Red) => 'P',
        (PieceKind::King, Color::Black) => 'k',
        (PieceKind::Advisor, Color::Black) => 'a',
        (PieceKind::Elephant, Color::Black) => 'b',
        (PieceKind::Horse, Color::Black) => 'n',
        (PieceKind::Rook, Color::Black) => 'r',
        (PieceKind::Cannon, Color::Black) => 'c',
        (PieceKind::Pawn, Color::Black) => 'p',
    }
}

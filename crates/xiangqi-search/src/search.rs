//! 内置搜索引擎：迭代加深 α-β 剪枝 + 置换表 + 杀手着法 + 吃子优先排序
//!
//! 确定性：不使用随机，同局面同深度同结果。
//! 超时：按节点预算检查，超时返回上一层完整迭代的最佳结果。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use xiangqi_core::board::Board;
use xiangqi_core::types::Move;

use crate::eval::{piece_value, Evaluator, MATE};

/// 搜索配置（预留，未来难度细分）
#[derive(Debug, Clone, Copy)]
pub struct SearchConfig {
    pub max_depth: u8, // 1..=6，对应难度档
}

/// 搜索报告
#[derive(Debug, Clone)]
pub struct SearchReport {
    /// 最佳着；`None` = 轮走方无着可走（将死/困毙）
    pub best_move: Option<Move>,
    /// 走子方视角得分（红方为正），近似 MATE 表示杀局
    pub score: i32,
    /// 实际完成的迭代深度
    pub depth: u8,
    pub nodes: u64,
    pub time_ms: u64,
    pub pv: Vec<Move>,
}

/// 内置引擎：以固定深度搜索
#[derive(Debug, Clone, Copy)]
pub struct BuiltinEngine {
    pub depth: u8,
}

impl BuiltinEngine {
    pub fn new(depth: u8) -> Self {
        BuiltinEngine {
            depth: depth.max(1),
        }
    }

    /// 不限时搜索（按深度跑完）
    pub fn best_move(&self, board: &Board) -> SearchReport {
        self.search(board, None)
    }

    /// 限时搜索：超时返回当前已完整完成的迭代层结果
    pub fn best_move_with_timeout(&self, board: &Board, ms: u64) -> SearchReport {
        self.search(board, Some(Duration::from_millis(ms)))
    }

    fn search(&self, board: &Board, timeout: Option<Duration>) -> SearchReport {
        let started = Instant::now();
        let deadline = timeout.map(|d| started + d);
        let eval = crate::eval::MaterialPositionEvaluator;
        let mut searcher = Searcher::new(board, &eval, deadline);
        let (score, best_move, depth, nodes) = searcher.iterative(self.depth);
        SearchReport {
            best_move,
            score,
            depth,
            nodes,
            time_ms: started.elapsed().as_millis() as u64,
            pv: best_move.map(|m| vec![m]).unwrap_or_default(),
        }
    }

    /// 根着法 Top-K 评分（走子方视角，降序，限时 ms；ms=0 不限时）。
    /// 供大模型引擎参谋制生成候选短名单。
    pub fn top_moves(&self, board: &Board, k: usize, ms: u64) -> Vec<(Move, i32)> {
        if k == 0 {
            return Vec::new();
        }
        let started = Instant::now();
        let deadline = if ms > 0 {
            Some(started + Duration::from_millis(ms))
        } else {
            None
        };
        let eval = crate::eval::MaterialPositionEvaluator;
        let mut searcher = Searcher::new(board, &eval, deadline);
        let _ = searcher.iterative(self.depth);
        searcher.root_scores.iter().take(k).copied().collect()
    }

    /// 单着评估：走完 mv 后以对方走子方视角搜 depth 层，返回**本方**视角评分。
    /// 供大模型引擎参谋制（gate 护航否决）评估模型所选着法。
    pub fn evaluate_move(&self, board: &Board, mv: Move, depth: u8) -> i32 {
        let child = board.make_move(mv);
        let eval = crate::eval::MaterialPositionEvaluator;
        let mut searcher = Searcher::new(&child, &eval, None);
        let (score, _, _, _) = searcher.iterative(depth.max(1));
        -score // 对方视角取负 = 本方视角
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
struct TTEntry {
    depth: u8,
    score: i32,
    flag: Flag,
    best: Move,
}

struct Searcher<'a> {
    /// 最近完成迭代层的根着法评分（走子方视角，降序）；供引擎参谋制使用
    root_scores: Vec<(Move, i32)>,

    board: &'a Board,
    eval: &'a dyn Evaluator,
    tt: HashMap<u64, TTEntry>,
    killers: [[Move; 2]; 64],
    nodes: u64,
    deadline: Option<Instant>,
    stopped: bool,
}

impl<'a> Searcher<'a> {
    fn new(board: &'a Board, eval: &'a dyn Evaluator, deadline: Option<Instant>) -> Self {
        Searcher {
            board,
            eval,
            tt: HashMap::new(),
            killers: [[Move::new(0, 0); 2]; 64],
            nodes: 0,
            deadline,
            stopped: false,
            root_scores: Vec::new(),
        }
    }

    /// 局面 Zobrist 风格积分哈希（含轮走方），确定性
    fn board_hash(&self, board: &Board) -> u64 {
        let mut h: u64 = 0x517c_c1b7_2722_0a95;
        for sq in 0..90u8 {
            if let Some(p) = board.piece_at(sq) {
                let code = (p.kind as u8) * 2 + p.color as u8 + 1; // 1..=14
                h = h
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add((code as u64) << 4 | sq as u64);
                h ^= 0xBF58_476D_1CE4_E5B9u64.wrapping_mul(sq as u64 + 1);
            }
        }
        if board.side_to_move == xiangqi_core::types::Color::Black {
            h ^= 0x94D0_49BB_1331_11EB;
        }
        h
    }

    /// 迭代加深；根节点按上一层分数排序
    fn iterative(&mut self, max_depth: u8) -> (i32, Option<Move>, u8, u64) {
        let root_moves = self.board.legal_moves();
        if root_moves.is_empty() {
            return (-MATE, None, 0, self.nodes);
        }

        let mut best_score = 0i32;
        let mut best_move: Option<Move> = None;
        let mut done_depth = 0u8;
        let mut last_scores: HashMap<Move, i32> = HashMap::new();
        // 第一层兜底排序：吃子优先
        let mut last_order: Vec<Move> = root_moves.to_vec();
        last_order.sort_by_key(|a| std::cmp::Reverse(self.capture_score(*a)));

        for d in 1..=max_depth {
            // 按上一层根分数降序
            let order: Vec<Move> = {
                let mut v = last_order.clone();
                v.sort_by(|a, b| {
                    let sa = last_scores.get(a).copied().unwrap_or(0);
                    let sb = last_scores.get(b).copied().unwrap_or(0);
                    sb.cmp(&sa)
                });
                v
            };
            let mut alpha = -MATE - 1;
            let beta = MATE + 1;
            let mut layer_best: Option<Move> = None;
            let mut layer_score = 0i32;
            let mut layer_scores: HashMap<Move, i32> = HashMap::new();

            for &mv in &order {
                let child = self.board.make_move(mv);
                let score = -self.negamax(&child, d - 1, -beta, -alpha, 1);
                if self.stopped {
                    break; // 本层被超时中止：放弃本层，保留上一层结果
                }
                layer_scores.insert(mv, score);
                if score > alpha {
                    alpha = score;
                    layer_best = Some(mv);
                    layer_score = score;
                }
            }

            if self.stopped {
                break;
            }
            best_score = layer_score;
            best_move = layer_best;
            done_depth = d;
            last_scores = layer_scores;
            last_order = order;
        }

        // 参谋制：收集最近完成层的根着法评分（走子方视角）
        self.root_scores = {
            let mut v: Vec<(Move, i32)> = last_scores
                .iter()
                .map(|(m, sc)| (*m, *sc))
                .collect();
            v.sort_by_key(|(_, sc)| std::cmp::Reverse(*sc));
            v
        };
        (best_score, best_move, done_depth, self.nodes)
    }

    /// 吃子分（MVV-LVA 简化）：价值×16 - 攻子价值
    fn capture_score(&self, mv: Move) -> i32 {
        match self.board.piece_at(mv.to) {
            None => 0,
            Some(victim) => {
                let v = piece_value(victim.kind) * 16;
                let a = self
                    .board
                    .piece_at(mv.from)
                    .map(|p| piece_value(p.kind))
                    .unwrap_or(0);
                v - a
            }
        }
    }

    /// 走法排序分：TT 最佳着 > 吃子 > 杀手
    fn move_score(&self, board: &Board, key: u64, mv: Move, ply: u8) -> i32 {
        if let Some(e) = self.tt.get(&key) {
            if e.best == mv {
                return 100_000_000;
            }
        }
        let mut s = 0;
        if let Some(victim) = board.piece_at(mv.to) {
            let v = piece_value(victim.kind) * 16;
            let a = board
                .piece_at(mv.from)
                .map(|p| piece_value(p.kind))
                .unwrap_or(0);
            s += 1_000_000 + v - a;
        }
        let k = self.killers[ply as usize];
        if k[0] == mv {
            s += 900_000;
        } else if k[1] == mv {
            s += 800_000;
        }
        s
    }

    fn negamax(&mut self, board: &Board, depth: u8, mut alpha: i32, mut beta: i32, ply: u8) -> i32 {
        self.nodes += 1;
        if self.nodes & 1023 == 0 {
            if let Some(dl) = self.deadline {
                if Instant::now() >= dl {
                    self.stopped = true;
                    return 0;
                }
            }
        }

        let key = self.board_hash(board);
        if let Some(e) = self.tt.get(&key) {
            if e.depth >= depth {
                match e.flag {
                    Flag::Exact => return e.score,
                    Flag::Lower => alpha = alpha.max(e.score),
                    Flag::Upper => beta = beta.min(e.score),
                }
                if alpha >= beta {
                    return e.score;
                }
            }
        }

        let moves = board.legal_moves();
        if moves.is_empty() {
            // 走子方无着可走（将死或困毙）：走子方负
            return -(MATE - ply as i32);
        }
        if depth == 0 {
            return self.eval.evaluate(board);
        }

        let mut ordered: Vec<(i32, Move)> = moves
            .iter()
            .map(|&m| (self.move_score(board, key, m, ply), m))
            .collect();
        ordered.sort_by_key(|(score, _)| std::cmp::Reverse(*score));

        let mut best = i32::MIN + 1;
        let mut best_move = ordered[0].1;
        let mut flag = Flag::Upper;
        for &(_, mv) in &ordered {
            let child = board.make_move(mv);
            let score = -self.negamax(&child, depth - 1, -beta, -alpha, ply + 1);
            if self.stopped {
                return best;
            }
            if score > best {
                best = score;
                best_move = mv;
                flag = Flag::Exact;
            }
            if best > alpha {
                alpha = best;
            }
            if alpha >= beta {
                flag = Flag::Lower;
                // 杀手着法记录（非吃子）
                if board.piece_at(mv.to).is_none() {
                    let k = &mut self.killers[ply as usize];
                    if k[0] != mv {
                        k[1] = k[0];
                        k[0] = mv;
                    }
                }
                break;
            }
        }

        if !self.stopped {
            self.tt.insert(
                key,
                TTEntry {
                    depth,
                    score: best,
                    flag,
                    best: best_move,
                },
            );
        }
        best
    }
}

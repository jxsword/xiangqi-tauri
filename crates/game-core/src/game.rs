//! 对局状态机：落子校验、终局判定、恢复重放、引擎走子

use xiangqi_core::board::Board;
use xiangqi_core::types::{Color, GameStatus, Move};

use crate::engine::{Engine, EngineError, EngineId, EngineOptions};
use crate::model::{GameMode, GameResult};

/// 对局错误
#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("FEN 无效：{0}")]
    InvalidFen(String),
    #[error("非法着 {mv}：{reason}")]
    IllegalMove { mv: String, reason: String },
    #[error("对局已结束：{0:?}")]
    GameOver(GameResult),
    #[error("引擎错误：{0}")]
    Engine(#[from] EngineError),
}

/// 落子后事件（供前端 Channel 推送）
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameEvent {
    /// 最后一着（UCCI）
    pub mv: String,
    /// 落子后 FEN
    pub fen: String,
    pub status: GameStatus,
    /// 终局结果（None = 进行中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    /// 轮到谁走（落子后）
    pub side_to_move: Color,
}

/// 一步棋的局面对局快照（用于重复局面/长将检测）
#[derive(Debug, Clone)]
struct Plv {
    /// 走完后的局面 FEN
    fen: String,
    /// 走出方
    mover: Color,
    /// 走出方是否将军（对方被将）
    check: bool,
}

/// 对局状态机
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    moves: Vec<Move>,
    /// 每步走完后的局面快照（索引 i 对应第 i 着，随 moves 同步增长）
    plies: Vec<Plv>,
    mode: GameMode,
    red_engine: Option<EngineId>,
    black_engine: Option<EngineId>,
    result: Option<GameResult>,
}

impl Game {
    /// 新建对局（start_fen 需合法）
    pub fn new(
        start_fen: &str,
        mode: GameMode,
        red_engine: Option<EngineId>,
        black_engine: Option<EngineId>,
    ) -> Result<Game, GameError> {
        let board = Board::parse_fen(start_fen).map_err(GameError::InvalidFen)?;
        Ok(Game {
            board,
            moves: Vec::new(),
            plies: Vec::new(),
            mode,
            red_engine,
            black_engine,
            result: None,
        })
    }

    /// 从存档恢复：start_fen + 着法谱重放（非法着拒绝）
    pub fn restore(
        start_fen: &str,
        mode: GameMode,
        red_engine: Option<EngineId>,
        black_engine: Option<EngineId>,
        moves: &[Move],
    ) -> Result<Game, GameError> {
        let mut g = Game::new(start_fen, mode, red_engine, black_engine)?;
        for mv in moves {
            g.play_move(*mv)?;
        }
        Ok(g)
    }

    pub fn board(&self) -> &Board {
        &self.board
    }
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }
    pub fn mode(&self) -> GameMode {
        self.mode
    }
    pub fn red_engine(&self) -> Option<EngineId> {
        self.red_engine
    }
    pub fn black_engine(&self) -> Option<EngineId> {
        self.black_engine
    }
    pub fn result(&self) -> Option<GameResult> {
        self.result
    }

    /// 中止对局（前端「停止」）：仅进行中可中止，返回是否成功
    pub fn abort(&mut self) -> bool {
        if self.result.is_none() {
            self.result = Some(GameResult::Aborted);
            true
        } else {
            false
        }
    }
    pub fn side_to_move(&self) -> Color {
        self.board.side_to_move
    }

    /// 当前是否轮到人走（无引擎控制该方）
    pub fn is_human_turn(&self) -> bool {
        self.result.is_none() && self.current_engine().is_none()
    }

    /// 当前轮走方配置的引擎（人走则 None）
    pub fn current_engine(&self) -> Option<EngineId> {
        if self.result.is_some() {
            return None;
        }
        match self.board.side_to_move {
            Color::Red => self.red_engine,
            Color::Black => self.black_engine,
        }
    }

    /// 落子：校验合法 → 走子 → 终局判定 → 事件
    pub fn play_move(&mut self, mv: Move) -> Result<GameEvent, GameError> {
        if let Some(r) = self.result {
            return Err(GameError::GameOver(r));
        }
        if !self.board.legal_moves().contains(&mv) {
            return Err(GameError::IllegalMove {
                mv: mv.to_ucci(),
                reason: "该着在当前局面不合法".into(),
            });
        }
        let mover = self.board.side_to_move;
        self.board = self.board.make_move(mv);
        self.moves.push(mv);
        // 快照：走出方是否将军 = 对方是否被将
        self.plies.push(Plv {
            fen: self.board.to_fen(),
            mover,
            check: self.board.is_in_check(mover.opposite()),
        });
        let status = self.board.game_status();

        // 规则级终局：将死/困毙优先；否则做重复局面/长将检测
        let mut result = match status {
            GameStatus::Checkmate(winner) => Some(GameResult::Win(winner)),
            GameStatus::Stalemate(loser) => Some(GameResult::Win(loser.opposite())),
            _ => None,
        };
        if result.is_none() {
            result = repetition_result(&self.plies);
        }
        if result.is_some() {
            self.result = result;
        }
        Ok(GameEvent {
            mv: mv.to_ucci(),
            fen: self.board.to_fen(),
            status,
            result,
            side_to_move: self.board.side_to_move,
        })
    }

    /// 引擎走子：取引擎着 → 统一落子（引擎非法着会被 play_move 拒绝）
    pub fn engine_move(
        &mut self,
        engine: &mut dyn Engine,
        opts: EngineOptions,
    ) -> Result<GameEvent, GameError> {
        let outcome = engine.best_move_with_history(&self.board, opts, self.moves())?;
        self.play_move(outcome.mv)
    }

    /// 机器对战推进一步：创建当前轮引擎并走子（不负责换引擎实例；由调用方管理）
    pub fn machine_move(
        &mut self,
        manager: &crate::engine::EngineManager,
        opts: EngineOptions,
    ) -> Result<(GameEvent, EngineId, Option<String>), GameError> {
        let id = self
            .current_engine()
            .ok_or_else(|| GameError::IllegalMove {
                mv: String::new(),
                reason: "当前轮到人走，不能由引擎代走".into(),
            })?;
        let mut engine = manager.create(id);
        let outcome = engine.best_move_with_history(&self.board, opts, self.moves())?;
        let event = self.play_move(outcome.mv)?;
        Ok((event, outcome.source, outcome.fallback_reason))
    }
}

/// 规则级重复局面检测：
/// 同一局面完整重现 ≥2 个周期（周期 2..=12 着）时：
///   - 循环内将军着法来自单一一方 → 该方长将判负；
///   - 无将军（普通重复）或双方互将军 → 判和。
/// 返回 None = 未构成可判定的循环。
fn repetition_result(plies: &[Plv]) -> Option<GameResult> {
    let n = plies.len();
    if n < 4 {
        return None;
    }
    for period in (2..=12usize).step_by(2) {
        if n < 2 * period {
            break;
        }
        if plies[n - 1].fen != plies[n - 1 - period].fen {
            continue;
        }
        // 验证整个周期是真正的循环：与再前一个周期逐着相同
        let mut cyclic = true;
        for i in 0..period {
            if plies[n - 1 - i].fen != plies[n - 1 - period - i].fen {
                cyclic = false;
                break;
            }
        }
        if !cyclic {
            continue;
        }
        // 循环内将军模式：将军着法若来自两方 → 相互长将（判和）
        let mut checkers: Option<Color> = None;
        let mut mixed = false;
        for p in &plies[n - 1 - period..n - 1] {
            if p.check {
                match checkers {
                    None => checkers = Some(p.mover),
                    Some(c) if c == p.mover => {}
                    _ => mixed = true,
                }
            }
        }
        if !mixed {
            if let Some(looser) = checkers {
                // 单一长将方 → 判负
                return Some(GameResult::Win(looser.opposite()));
            }
        }
        // 普通重复 / 相互长将 → 和棋
        return Some(GameResult::Draw);
    }
    None
}

#[cfg(test)]
mod repetition_tests {
    use super::*;

    fn plv(fen: &str, mover: Color, check: bool) -> Plv {
        Plv {
            fen: fen.into(),
            mover,
            check,
        }
    }

    #[test]
    fn repetition_simple_cycle_draw() {
        // A-B 往返 4 着，无将军 → 和棋
        let plies = vec![
            plv("f1", Color::Red, false),
            plv("f2", Color::Black, false),
            plv("f1", Color::Red, false),
            plv("f2", Color::Black, false),
        ];
        assert_eq!(repetition_result(&plies), Some(GameResult::Draw));
    }

    #[test]
    fn repetition_long_check_loser() {
        // 红每着将军、黑应将（非将军）→ 红长将判负
        let plies = vec![
            plv("f1", Color::Red, true),
            plv("f2", Color::Black, false),
            plv("f1", Color::Red, true),
            plv("f2", Color::Black, false),
        ];
        assert_eq!(
            repetition_result(&plies),
            Some(GameResult::Win(Color::Black))
        );
    }

    #[test]
    fn repetition_mutual_check_draw() {
        // 双方互将军 → 和棋
        let plies = vec![
            plv("f1", Color::Red, true),
            plv("f2", Color::Black, true),
            plv("f1", Color::Red, true),
            plv("f2", Color::Black, true),
        ];
        assert_eq!(repetition_result(&plies), Some(GameResult::Draw));
    }

    #[test]
    fn repetition_no_cycle_none() {
        let plies = vec![plv("f1", Color::Red, false), plv("f2", Color::Black, false)];
        assert_eq!(repetition_result(&plies), None);
    }

    #[test]
    fn repetition_period4_cycle_draw() {
        // 4 着周期循环（无将军）→ 和棋
        let plies = vec![
            plv("f1", Color::Red, false),
            plv("f2", Color::Black, false),
            plv("f3", Color::Red, false),
            plv("f4", Color::Black, false),
            plv("f1", Color::Red, false),
            plv("f2", Color::Black, false),
            plv("f3", Color::Red, false),
            plv("f4", Color::Black, false),
        ];
        assert_eq!(repetition_result(&plies), Some(GameResult::Draw));
    }

    #[test]
    fn repetition_not_enough_plies_none() {
        let plies = vec![plv("f1", Color::Red, false), plv("f2", Color::Black, false), plv("f1", Color::Red, false)];
        assert_eq!(repetition_result(&plies), None);
    }

    #[test]
    fn abort_marks_aborted_and_locks() {
        let start = Board::start_position().to_fen();
        let mut g = Game::new(
            &start,
            GameMode::HumanVsMachine,
            None,
            Some(EngineId::Builtin { depth: 2 }),
        )
        .unwrap();
        assert!(g.abort(), "进行中可中止");
        assert_eq!(g.result(), Some(GameResult::Aborted));
        let mv = g.board().legal_moves()[0];
        assert!(g.play_move(mv).is_err(), "已中止后不可再落子");
        assert!(!g.abort(), "已中止不可重复中止");
    }
}

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

/// 对局状态机
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    moves: Vec<Move>,
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
        self.board = self.board.make_move(mv);
        self.moves.push(mv);
        let status = self.board.game_status();

        let result = match status {
            GameStatus::Checkmate(winner) => Some(GameResult::Win(winner)),
            GameStatus::Stalemate(loser) => Some(GameResult::Win(loser.opposite())),
            _ => None,
        };
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
        let outcome = engine.best_move(&self.board, opts)?;
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
        let outcome = engine.best_move(&self.board, opts)?;
        let event = self.play_move(outcome.mv)?;
        Ok((event, outcome.source, outcome.fallback_reason))
    }
}

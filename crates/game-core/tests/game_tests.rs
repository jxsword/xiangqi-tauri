//! 对局状态机集成测试：落子校验、终局、恢复、人机回合、机器对战完整对局

use game_core::engine::{EngineId, EngineManager, EngineOptions};
use game_core::model::{GameMode, GameResult};
use game_core::{Engine, Game, GameError};
use xiangqi_core::types::{Color, GameStatus, Move};

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn human_vs_machine() -> Game {
    Game::new(
        START_FEN,
        GameMode::HumanVsMachine,
        None,
        Some(EngineId::Builtin { depth: 1 }),
    )
    .unwrap()
}

fn machine_vs_machine() -> Game {
    Game::new(
        START_FEN,
        GameMode::MachineVsMachine,
        Some(EngineId::Builtin { depth: 1 }),
        Some(EngineId::Builtin { depth: 1 }),
    )
    .unwrap()
}

#[test]
fn new_game_initial_state() {
    let g = human_vs_machine();
    assert_eq!(g.side_to_move(), Color::Red);
    assert!(g.moves().is_empty());
    assert!(g.result().is_none());
    assert!(g.is_human_turn(), "红方为人应先走");
}

#[test]
fn play_legal_and_illegal_moves() {
    let mut g = human_vs_machine();
    // 非法着拒绝且棋盘不变
    let bad = Move::from_ucci("h8e8").unwrap(); // 黑方棋子红方不能走
    assert!(matches!(
        g.play_move(bad),
        Err(GameError::IllegalMove { .. })
    ));
    assert_eq!(g.moves().len(), 0);

    // 合法着生效
    let mv = Move::from_ucci("h2e2").unwrap();
    let ev = g.play_move(mv).unwrap();
    assert_eq!(ev.side_to_move, Color::Black);
    assert_eq!(ev.status, GameStatus::Ongoing);
    assert!(ev.result.is_none());
    assert_eq!(g.moves().len(), 1);
    assert!(!g.is_human_turn(), "轮黑，黑方为引擎");
    assert_eq!(g.current_engine(), Some(EngineId::Builtin { depth: 1 }));
}

#[test]
fn game_ended_blocks_further_moves() {
    // 轮黑已被将死（合法杀形，e5 兵遮视线）
    let fen = "3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 b - - 0 1";
    let g = Game::new(fen, GameMode::HumanVsMachine, None, None).unwrap();
    assert_eq!(g.board().game_status(), GameStatus::Checkmate(Color::Red));

    // 直接构造终局再走一步应报错
    let mut g2 = Game::new(fen, GameMode::HumanVsMachine, None, None).unwrap();
    // 黑方已无合法着；红方若引擎代走也因已结束而报错
    let ev = g2.machine_move(&EngineManager, EngineOptions::default());
    assert!(ev.is_err(), "对局结束后不应再有走子");
    let _ = g;
}

#[test]
fn restore_replays_moves() {
    let moves: Vec<Move> = vec![
        Move::from_ucci("h2e2").unwrap(),
        Move::from_ucci("h7e7").unwrap(),
        Move::from_ucci("h0g2").unwrap(),
    ];
    let mut g = Game::restore(START_FEN, GameMode::HumanVsHuman, None, None, &moves).unwrap();
    assert_eq!(g.moves().len(), 3);
    assert_eq!(g.side_to_move(), Color::Black);
    assert_eq!(g.board().to_fen(), {
        let mut b = xiangqi_core::board::Board::start_position();
        for mv in &moves {
            b = b.make_move(*mv);
        }
        b.to_fen()
    });
    // 继续走子正常（3 步后轮黑，走黑马）
    let ev = g.play_move(Move::from_ucci("b9c7").unwrap()).unwrap();
    assert!(ev.fen.len() > 10);
}

#[test]
fn restore_rejects_illegal_sequence() {
    let moves = vec![Move::from_ucci("h8e8").unwrap()]; // 首着即非法
    assert!(Game::restore(START_FEN, GameMode::HumanVsHuman, None, None, &moves).is_err());
}

#[test]
fn human_vs_machine_turn_cycle() {
    let mut g = human_vs_machine();
    assert!(g.is_human_turn());
    // 人走红
    g.play_move(Move::from_ucci("h2e2").unwrap()).unwrap();
    assert!(!g.is_human_turn());
    // 引擎走黑（内置深度1）
    let (ev, source, fb) = g
        .machine_move(&EngineManager, EngineOptions::default())
        .unwrap();
    assert!(matches!(source, EngineId::Builtin { .. }));
    assert!(fb.is_none());
    assert!(ev.side_to_move == Color::Red || ev.result.is_some());
    // 引擎走后回到人走（若未终局）
    if g.result().is_none() {
        assert!(g.is_human_turn());
    }
}

#[test]
fn machine_vs_machine_full_game() {
    // 机器对战：浅深度引擎互弈，直至终局或步数上限
    let mut g = machine_vs_machine();
    let manager = EngineManager;
    let opts = EngineOptions { think_ms: 0 };
    let mut steps = 0u32;
    let max_steps = 200;
    loop {
        if g.result().is_some() {
            break;
        }
        let (ev, _, _) = g.machine_move(&manager, opts).unwrap();
        assert!(matches!(
            ev.status,
            GameStatus::Ongoing
                | GameStatus::Check
                | GameStatus::Checkmate(_)
                | GameStatus::Stalemate(_)
        ));
        // 每一着都必须是合法着：play_move 内部已校验
        steps += 1;
        assert!(steps <= max_steps, "机器对战应在 {max_steps} 步内终局");
    }
    let result = g.result().expect("机器对战必须终局");
    match result {
        GameResult::Win(c) => assert!(c == Color::Red || c == Color::Black),
        GameResult::Draw => {}
    }
    // 终局后棋盘状态一致
    let status = g.board().game_status();
    assert!(matches!(
        status,
        GameStatus::Checkmate(_) | GameStatus::Stalemate(_)
    ));
}

#[test]
fn machine_move_requires_engine_side() {
    // 人走方（红方为人）调用 machine_move 应报错
    let mut g = human_vs_machine();
    let r = g.machine_move(&EngineManager, EngineOptions::default());
    assert!(r.is_err());
}

#[test]
fn builtin_engine_returns_legal_move() {
    let mut engine = game_core::BuiltinEngineAdapter::new(2);
    let b = xiangqi_core::board::Board::start_position();
    let out = engine.best_move(&b, EngineOptions::default()).unwrap();
    assert!(b.legal_moves().contains(&out.mv));
    assert_eq!(engine.name(), "内置引擎·深度2");
}

#[test]
fn builtin_engine_no_legal_move() {
    // 轮黑无合法着：引擎应返回 NoLegalMove
    let fen = "3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 b - - 0 1";
    let b = xiangqi_core::board::Board::parse_fen(fen).unwrap();
    let mut engine = game_core::BuiltinEngineAdapter::new(2);
    assert!(engine.best_move(&b, EngineOptions::default()).is_err());
}

#[test]
fn engine_manager_creates_adapters() {
    let m = EngineManager;
    let e = m.create(EngineId::Builtin { depth: 3 });
    assert!(matches!(e.id(), EngineId::Builtin { depth: 3 }));
    let e2 = m.create(EngineId::Llm);
    assert!(matches!(e2.id(), EngineId::Llm));
    let e3 = m.create(EngineId::Pikafish);
    assert!(matches!(e3.id(), EngineId::Pikafish));
}

//! 搜索引擎集成测试：确定性、评估对称性、杀局感知、超时、无着可走

use xiangqi_core::board::Board;
use xiangqi_core::types::Move;
use xiangqi_search::eval::{MaterialPositionEvaluator, MATE};
use xiangqi_search::search::BuiltinEngine;
use xiangqi_search::Evaluator;

/// 将死局面（黑将被困，轮黑）：黑将 e9；红车 d9 将军（d8 车保护）、
/// 红车 d8 控 e8/f8、红马 g7 控 f9/e8、e5 兵遮将帅视线 → 黑将无合法着
const MATE_FEN_BLACK_MOVES: &str = "3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 b";

/// 同棋形轮红：红方保持杀形，搜索应判红方接近必胜
const MATE_FEN_RED_MOVES: &str = "3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 w";

#[test]
fn determinism_same_position_same_move() {
    let b = Board::start_position();
    let e = BuiltinEngine::new(3);
    let r1 = e.best_move(&b);
    let r2 = e.best_move(&b);
    assert_eq!(r1.best_move, r2.best_move, "同局面同深度必须同结果");
    assert_eq!(r1.score, r2.score);
    let mv = r1.best_move.expect("初始局面必有最佳着");
    assert!(b.legal_moves().contains(&mv), "搜索引擎必须只返回合法着");
}

#[test]
fn initial_position_returns_legal_move() {
    let b = Board::start_position();
    let e = BuiltinEngine::new(1);
    let r = e.best_move(&b);
    let mv = r.best_move.expect("初始局面必有最佳着");
    assert!(b.legal_moves().contains(&mv));
    assert_eq!(r.depth, 1);
}

#[test]
fn search_finds_winning_position_score() {
    // 轮红、黑将已被困死：搜索应给出接近 MATE 的得分
    let b = Board::parse_fen(MATE_FEN_RED_MOVES).unwrap();
    let e = BuiltinEngine::new(2);
    let r = e.best_move(&b);
    assert!(r.best_move.is_some());
    assert!(
        r.score > MATE / 2,
        "红方优势应接近将死得分，实际 {}",
        r.score
    );
}

#[test]
fn no_moves_returns_none_and_negative_mate() {
    // 轮黑、黑无合法着：直接返回 None 与负 MATE
    let b = Board::parse_fen(MATE_FEN_BLACK_MOVES).unwrap();
    assert!(b.legal_moves().is_empty(), "该局面黑方应无合法着");
    let e = BuiltinEngine::new(3);
    let r = e.best_move(&b);
    assert_eq!(r.best_move, None);
    assert!(r.score <= -MATE / 2, "走子方应判负，实际 {}", r.score);
}

#[test]
fn eval_symmetry_initial_is_zero() {
    let eval = MaterialPositionEvaluator;
    let b = Board::start_position();
    assert_eq!(eval.evaluate(&b), 0);
}

#[test]
fn eval_mirror_negates() {
    // 上下镜像 + 颜色翻转：x（黑车 h9、红车 a0、红帅 e0，轮红）
    //                    y（黑帅 e9、红车 h0、黑车 a9，轮黑）
    // 黑方位置分取 rank 镜像，与红方原表抵消 → eval 互为相反数。
    let eval = MaterialPositionEvaluator;
    let x = Board::parse_fen("7r1/9/9/9/9/9/9/9/9/R3K4 w").unwrap();
    let y = Board::parse_fen("r3k4/9/9/9/9/9/9/9/9/7R1 b").unwrap();
    assert_eq!(eval.evaluate(&x), -eval.evaluate(&y));
}

#[test]
fn timeout_returns_quickly() {
    let b = Board::start_position();
    let e = BuiltinEngine::new(6);
    let started = std::time::Instant::now();
    let r = e.best_move_with_timeout(&b, 1);
    let elapsed = started.elapsed().as_millis();
    assert!(r.best_move.is_some() || b.legal_moves().is_empty());
    assert!(elapsed < 5_000, "限时 1ms 搜索应快速返回，实际 {elapsed}ms");
    assert!(r.depth >= 1 || b.legal_moves().is_empty());
}

#[test]
fn shallow_vs_deep_consistency() {
    // 各深度不崩溃且着法合法
    let b = Board::parse_fen("4k4/9/9/9/9/9/9/9/2R6/4K4 w").unwrap();
    for d in 1..=3u8 {
        let e = BuiltinEngine::new(d);
        let r = e.best_move(&b);
        let mv = r.best_move.expect("有合法着");
        assert!(b.legal_moves().contains(&mv), "深度 {d} 着法必须合法");
    }
}

#[test]
fn ucci_roundtrip_of_best_move() {
    let b = Board::start_position();
    let e = BuiltinEngine::new(2);
    let r = e.best_move(&b);
    let mv = r.best_move.unwrap();
    let u = mv.to_ucci();
    assert_eq!(u.len(), 4);
    assert_eq!(Move::from_ucci(&u), Some(mv));
}

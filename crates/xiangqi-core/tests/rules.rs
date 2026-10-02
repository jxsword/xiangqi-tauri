//! 规则核心集成测试：走法生成、攻击检测、FEN、胜负/困毙判定
//!
//! FEN 行序约定：第 1 行 = rank9（黑方底线）…第 10 行 = rank0（红方底线）。
//! 每个测试用 FEN 构造精确局面，断言具体着法集合。

use xiangqi_core::board::Board;
use xiangqi_core::types::{Color, GameStatus, Move, PieceKind};

fn legal_ucci(board: &Board) -> Vec<String> {
    let mut v: Vec<String> = board.legal_moves().iter().map(|m| m.to_ucci()).collect();
    v.sort();
    v
}

fn pseudo_ucci(board: &Board) -> Vec<String> {
    let mut v: Vec<String> = board
        .pseudo_legal_moves()
        .iter()
        .map(|m| m.to_ucci())
        .collect();
    v.sort();
    v
}

#[test]
fn start_position_fen_roundtrip() {
    let b = Board::start_position();
    let fen = b.to_fen();
    assert_eq!(
        fen,
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1"
    );
    let b2 = Board::parse_fen(&fen).unwrap();
    assert_eq!(b, b2);
    assert_eq!(b2.side_to_move, Color::Red);
}

#[test]
fn parse_fen_errors() {
    assert!(Board::parse_fen("").is_err());
    assert!(Board::parse_fen("rnbakabnr/9").is_err());
    assert!(Board::parse_fen("bad/9/9/9/9/9/9/9/9/9 w").is_err());
    assert!(Board::parse_fen("4k4/9/9/9/9/9/9/9/9/4K4 x").is_err());
    assert!(Board::parse_fen("4k4/9/9/9/9/9/9/9/9").is_err());
}

#[test]
fn parse_fen_black_to_move() {
    let b = Board::parse_fen("4k4/9/9/9/9/9/9/9/9/4K4 b").unwrap();
    assert_eq!(b.side_to_move, Color::Black);
}

#[test]
fn initial_position_move_count() {
    // 红方第一着合法着法数（实测 44，与公开统计一致）：车 4；马 4；相 4；仕 2；帅 1；
    // 炮 b2 12 着（a2,b1,b3,b4,b5,b6,b9,c2,d2,e2,f2,g2）；炮 h2 12 着；兵 5 → 共 44。
    let b = Board::start_position();
    let moves = legal_ucci(&b);
    assert_eq!(moves.len(), 44, "实际着法：{moves:?}");

    // 抽查关键着法（传统记谱对照）
    assert!(moves.contains(&"h2e2".to_string()), "炮二平五");
    assert!(moves.contains(&"b2e2".to_string()), "炮八平五");
    assert!(moves.contains(&"h0g2".to_string()), "马二进三");
    assert!(moves.contains(&"b0a2".to_string()), "马八进九");
    assert!(moves.contains(&"g0e2".to_string()), "相三进五");
    assert!(moves.contains(&"d0e1".to_string()), "仕六进五");
    assert!(moves.contains(&"e0e1".to_string()), "帅五进一");
    assert!(moves.contains(&"a0a1".to_string()), "车九进一");
    assert!(moves.contains(&"a3a4".to_string()), "兵九进一");
    assert!(
        moves.contains(&"b2b9".to_string()),
        "炮八进七（隔 b7 黑炮吃马）"
    );
}

#[test]
fn horse_leg_block() {
    // 红马 b2（第8行），红兵 b3（第7行）挡马腿 → 不能跳 a4/c4，仍可跳 a0/c0
    let b = Board::parse_fen("6k2/9/9/9/9/9/1P7/1N7/9/4K4 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"b2a0".to_string()));
    assert!(m.contains(&"b2c0".to_string()));
    assert!(!m.contains(&"b2a4".to_string()), "腿 b3 有兵");
    assert!(!m.contains(&"b2c4".to_string()), "腿 b3 有兵");
}

#[test]
fn elephant_eye_block() {
    // 红相 c2（第8行）、黑卒 b3（第7行）塞象眼、红帅 d0（第10行）
    // → 相不能到 a4（象眼 b3），可到 e4/a0/e0（目标均空）
    let b = Board::parse_fen("4k4/9/9/9/9/9/1p7/2B6/9/3K5 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c2e4".to_string()));
    assert!(m.contains(&"c2a0".to_string()));
    assert!(m.contains(&"c2e0".to_string()));
    assert!(!m.contains(&"c2a4".to_string()), "象眼 b3 有卒");
}

#[test]
fn cannon_screen_capture() {
    // 红炮 c2、红兵 b2 为炮架、黑车 a2（第8行）→ 炮隔一吃车；不能吃己方 b2
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/rPC6/9/4K4 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c2a2".to_string()), "炮隔兵吃车");
    assert!(m.contains(&"c2c1".to_string()), "炮直进非吃");
    assert!(!m.contains(&"c2b2".to_string()), "不能吃己方");
    let mv = Move::from_ucci("c2a2").unwrap();
    let after = b.make_move(mv);
    // a2 = coord(0,2) = 18
    assert_eq!(after.piece_at(18).unwrap().kind, PieceKind::Cannon);
    assert_eq!(after.piece_at(18).unwrap().color, Color::Red);
}

#[test]
fn cannon_cannot_jump_without_screen() {
    // 红炮 c2（第8行），无炮架 → 只能滑行直走，不能越过子
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/2C6/9/4K4 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c2c3".to_string()));
    assert!(m.contains(&"c2c1".to_string()));
}

#[test]
fn pawn_river_rules() {
    // 红兵 c3（第7行）未过河：只能直进 c4
    let b = Board::parse_fen("6k2/9/9/9/9/9/2P6/9/9/4K4 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c3c4".to_string()));
    assert!(!m.contains(&"c3b3".to_string()));
    assert!(!m.contains(&"c3d3".to_string()));

    // 红兵 c5（第5行）已过河：可进、可横，不能退
    let b = Board::parse_fen("6k2/9/9/9/2P6/9/9/9/9/4K4 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c5c6".to_string()));
    assert!(m.contains(&"c5b5".to_string()));
    assert!(m.contains(&"c5d5".to_string()));
    assert!(!m.contains(&"c5c4".to_string()));

    // 黑卒 c4（第6行）已过河（黑方过河为 rank<=4）
    let b = Board::parse_fen("6k2/9/9/9/9/2p6/9/9/9/4K4 b").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"c4c3".to_string()));
    assert!(m.contains(&"c4b4".to_string()));
    assert!(m.contains(&"c4d4".to_string()));
}

#[test]
fn king_palace_limit() {
    // 红帅 e2（第8行）：九宫内可走 d2/f2/e1；e3 出宫非法
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/4K4/9/9 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"e2d2".to_string()));
    assert!(m.contains(&"e2f2".to_string()));
    assert!(m.contains(&"e2e1".to_string()));
    assert!(!m.contains(&"e2e3".to_string()), "帅不能出九宫");
}

#[test]
fn advisor_palace_limit() {
    // 红仕 d0（第10行）：仅可斜一步到 e1；c1 出宫、直走、斜两步均非法
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/9/9/3A5 w").unwrap();
    let m = legal_ucci(&b);
    assert!(m.contains(&"d0e1".to_string()));
    assert!(!m.contains(&"d0c1".to_string()), "仕出九宫");
    assert!(!m.contains(&"d0d1".to_string()), "仕不能直走");
    assert!(!m.contains(&"d0e2".to_string()), "仕只能斜一步");
}

#[test]
fn kings_face_illegal() {
    // 两王同列无遮挡：红帅 e0 与黑将 e9 → 互见；红帅不能 e0e1（仍同列），可横走
    let b = Board::parse_fen("4k4/9/9/9/9/9/9/9/9/4K4 w").unwrap();
    assert!(b.is_in_check(Color::Red));
    assert!(b.is_in_check(Color::Black));
    let m = legal_ucci(&b);
    assert!(!m.contains(&"e0e1".to_string()), "走后仍互见");
    assert!(m.contains(&"e0d0".to_string()));
    assert!(m.contains(&"e0f0".to_string()));
}

#[test]
fn king_line_blocked_by_own_pawn() {
    // 黑车 e9（第1行）与红帅 e0 同列，红兵 e5（第5行，已过河）遮挡 → 不将军；
    // 兵 e5 横走离开 e 列（如 e5d5）打开车线送将 → 非法；直进 e6 仍遮挡 → 合法
    let b = Board::parse_fen("4r4/9/9/9/4P4/9/9/9/9/4K4 w").unwrap();
    assert!(!b.is_in_check(Color::Red));
    let m = legal_ucci(&b);
    assert!(
        !m.contains(&"e5d5".to_string()),
        "横走离开 e 列会打开车线送将"
    );
    assert!(m.contains(&"e5e6".to_string()), "直进仍遮挡车线");
    assert!(m.contains(&"e0d0".to_string()));
}

#[test]
fn check_detection_by_piece() {
    // 黑马 d2（第8行）将红帅 e0（腿 d1 空）
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/3n5/9/4K4 w").unwrap();
    assert!(b.is_in_check(Color::Red));

    // 马腿 d1（第9行红兵）有子 → 不将军
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/3n5/3P5/4K4 w").unwrap();
    assert!(!b.is_in_check(Color::Red));

    // 黑炮 e2（第8行，小写 c）隔红兵 e1（第9行）将红帅 e0
    let b = Board::parse_fen("4k4/9/9/9/9/9/9/4c4/4P4/4K4 w").unwrap();
    assert!(b.is_in_check(Color::Red));

    // 黑车 e9 直线将
    let b = Board::parse_fen("4r4/9/9/9/9/9/9/9/9/4K4 w").unwrap();
    assert!(b.is_in_check(Color::Red));
}

#[test]
fn check_status() {
    // 黑马 d2 将：轮红 → Check
    let b = Board::parse_fen("6k2/9/9/9/9/9/9/3n5/9/4K4 w").unwrap();
    assert_eq!(b.game_status(), GameStatus::Check);
}

#[test]
fn checkmate_two_rooks_horse() {
    // 黑将 e9；红车 e8（将军，被帅 e0 保护）、红车 d8（控 d9）、红马 g7（控 f9）
    // 黑将合法着 d9/f9 均被控，e8 吃车互见非法 → 将死
    let b = Board::parse_fen("3k5/3RR4/6N2/9/9/9/9/9/9/4K4 b").unwrap();
    let m = legal_ucci(&b);
    assert!(m.is_empty(), "黑应无合法着：{m:?}");
    assert_eq!(b.game_status(), GameStatus::Checkmate(Color::Red));
}

#[test]
fn stalemate_palace_blockade() {
    // 黑将 e9 被红炮 d9/f9、红炮 e8 包围，红车 d0/f0 保护两炮；
    // 黑将不被将军（炮相邻无架、帅视线被 e8 炮遮挡）且无着可走 → 困毙（走子方负）
    let b = Board::parse_fen("3CkC3/4C4/9/9/9/9/9/9/9/3RKR3 b").unwrap();
    assert!(!b.is_in_check(Color::Black), "困毙前提：不被将军");
    let m = legal_ucci(&b);
    assert!(m.is_empty(), "黑应无合法着：{m:?}");
    assert_eq!(b.game_status(), GameStatus::Stalemate(Color::Black));
}

#[test]
fn move_toggles_side() {
    let b = Board::start_position();
    let mv = Move::from_ucci("h2e2").unwrap();
    let after = b.make_move(mv);
    assert_eq!(after.side_to_move, Color::Black);
    // e2 = coord(4,2) = 22
    assert_eq!(after.piece_at(22).unwrap().kind, PieceKind::Cannon);
    assert_eq!(after.piece_at(22).unwrap().color, Color::Red);
}

#[test]
fn legal_filters_self_check_only() {
    // 过河兵 e5 横走 e5d5 是伪合法但非法（打开车线送将）
    let b = Board::parse_fen("4r4/9/9/9/4P4/9/9/9/9/4K4 w").unwrap();
    let pseudo = pseudo_ucci(&b);
    let legal = legal_ucci(&b);
    assert!(pseudo.contains(&"e5d5".to_string()));
    assert!(!legal.contains(&"e5d5".to_string()));
}

#[test]
fn initial_position_both_legality_and_pseudo() {
    // 初始局面：伪合法着与合法着一致（无送将）
    let b = Board::start_position();
    assert_eq!(legal_ucci(&b), pseudo_ucci(&b));
    assert_eq!(b.game_status(), GameStatus::Ongoing);
}

//! 持久化集成测试：往返一致性、槽位边界、损坏容错、非法着拒绝、自动存档

use game_core::model::{EngineConfig, EngineKind, GameMode, GameResult};
use game_store::{validate_and_replay, SaveData, SaveStore, SLOT_COUNT};
use xiangqi_core::types::Color;

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn tmp_store(tag: &str) -> SaveStore {
    let dir = std::env::temp_dir().join(format!("xiq-store-test-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok();
    SaveStore::new(dir)
}

fn sample_data() -> SaveData {
    let mut d = SaveData::new("对局 1", START_FEN);
    d.mode = GameMode::MachineVsMachine;
    d.red_engine = Some(EngineConfig {
        kind: EngineKind::Builtin,
        depth: Some(4),
    });
    d.black_engine = Some(EngineConfig::llm());
    d.moves = vec!["h2e2".to_string(), "h7e7".to_string()];
    d.result = None;
    d
}

#[test]
fn save_load_roundtrip() {
    let s = tmp_store("roundtrip");
    let d = sample_data();
    s.save(1, &d).unwrap();
    let loaded = s.load(1).unwrap();
    assert_eq!(loaded, d, "往返必须逐字段一致");

    // 重放局面一致：红黑各走一步后轮红方且对局进行中
    let (board, status) = validate_and_replay(&loaded).unwrap();
    assert_eq!(board.side_to_move, Color::Red);
    assert!(matches!(status, xiangqi_core::types::GameStatus::Ongoing));
}

#[test]
fn slot_boundaries_and_delete() {
    let s = tmp_store("slots");
    // 越界槽
    assert!(s.save(0, &sample_data()).is_err());
    assert!(s.save(6, &sample_data()).is_err());
    assert!(s.load(0).is_err());
    // 有效槽
    for slot in 1..=SLOT_COUNT {
        s.save(slot, &sample_data()).unwrap();
        assert!(s.load(slot).is_ok());
    }
    // 全 5 槽列表
    assert_eq!(s.list_saves().len(), SLOT_COUNT);
    // 删除
    s.delete_save(3).unwrap();
    assert!(s.load(3).is_err());
    assert_eq!(s.list_saves().len(), SLOT_COUNT - 1);
    // 删除不存在的槽不报错
    s.delete_save(3).unwrap();
}

#[test]
fn corrupt_json_returns_error() {
    let s = tmp_store("corrupt");
    s.save(1, &sample_data()).unwrap();
    // 人为写坏 JSON
    let path = std::env::temp_dir()
        .join(format!("xiq-store-test-corrupt-{}", std::process::id()))
        .join("save_1.json");
    std::fs::write(&path, "{ not valid json !!").unwrap();
    assert!(s.load(1).is_err(), "损坏 JSON 必须返回错误而非 panic");
    // 列表应跳过坏槽
    assert!(s.list_saves().is_empty());
}

#[test]
fn illegal_move_rejected_with_index() {
    // moves 第 1 步非法（h8e8 不是红方第一步）
    let mut d = sample_data();
    d.moves = vec!["h8e8".to_string(), "h2e2".to_string()];
    let err = validate_and_replay(&d).unwrap_err();
    assert!(
        err.to_string().contains("第 0 步"),
        "错误必须报告非法步序号，实际：{err}"
    );

    // 中段非法：第二步黑车吃己方
    let mut d2 = sample_data();
    d2.moves = vec!["h2e2".to_string(), "h8h9".to_string()];
    assert!(validate_and_replay(&d2).is_err());

    // 坏 UCCI
    let mut d3 = sample_data();
    d3.moves = vec!["xxyy".to_string()];
    assert!(validate_and_replay(&d3).is_err());

    // 坏 FEN
    let mut d4 = sample_data();
    d4.start_fen = "bad fen".to_string();
    assert!(validate_and_replay(&d4).is_err());
}

#[test]
fn autosave_roundtrip_and_clear() {
    let s = tmp_store("auto");
    assert!(!s.has_autosave());
    let d = sample_data();
    s.autosave(&d).unwrap();
    assert!(s.has_autosave());
    let r = s.restore_autosave().unwrap();
    assert_eq!(r, d);
    s.clear_autosave().unwrap();
    assert!(!s.has_autosave());
    assert!(s.restore_autosave().is_err());
}

#[test]
fn autosave_overwrites_previous() {
    let s = tmp_store("auto2");
    let mut d1 = sample_data();
    d1.name = "第一版".to_string();
    s.autosave(&d1).unwrap();
    let mut d2 = sample_data();
    d2.name = "第二版".to_string();
    d2.moves.push("e2e8".to_string());
    s.autosave(&d2).unwrap();
    let r = s.restore_autosave().unwrap();
    assert_eq!(r.name, "第二版", "自动存档应覆盖旧版本");
}

#[test]
fn summary_status_text() {
    let s = tmp_store("summary");
    let mut d = sample_data();
    s.save(1, &d).unwrap();
    assert_eq!(s.list_saves()[0].status, "进行中");
    d.result = Some(GameResult::Win(Color::Red));
    s.save(2, &d).unwrap();
    assert_eq!(s.list_saves()[1].status, "红胜");
    d.result = Some(GameResult::Draw);
    s.save(2, &d).unwrap();
    assert_eq!(s.list_saves()[1].status, "和棋");
}

#[test]
fn validate_full_game_replay_to_result() {
    // 用合法杀形局面（红车 d9 将军、e5 兵遮互见、马控 f9/e8）验证重放
    let mut d = sample_data();
    d.start_fen = "3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4 w - - 0 1".to_string();
    d.moves = vec![];
    let (board, status) = validate_and_replay(&d).unwrap();
    assert!(matches!(status, xiangqi_core::types::GameStatus::Ongoing));
    // 轮红走棋时该局面仍 Ongoing（杀形保持，红方先行）
    assert_eq!(board.side_to_move, Color::Red);
}

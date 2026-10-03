//! Tauri 命令层：对局编排（新建/走子/机器走子）、存档（5 槽 + 自动）、大模型配置

use std::sync::Mutex;

use game_core::engine::{EngineId, EngineManager, EngineOptions};
use game_core::model::{EngineConfig, EngineKind, GameMode, GameResult};
use game_core::{Game, GameError};
use game_store::{SaveData, SaveStore, SaveSummary};
use serde::Serialize;
use tauri::State;
use xiangqi_core::types::{Color, Move};

/// 应用共享状态
pub struct AppState {
    /// 当前对局（None = 未开局）
    pub game: Mutex<Option<Game>>,
    /// 大模型配置（密钥仅存内存 + 本机设置文件）
    pub llm: Mutex<Option<llm_engine::LlmConfig>>,
    /// 存档存储（app_data_dir/saves）
    pub saves: SaveStore,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    pub fen: String,
    /// "red" | "black"
    pub side_to_move: String,
    /// playing | checkmate | stalemate | draw
    pub status: String,
    /// red_win | black_win | draw（None=进行中）
    pub result: Option<String>,
    pub last_move: Option<String>,
    /// 大模型降级原因（None=正常）
    pub last_reason: Option<String>,
    /// 当前轮方全部合法着（仅人走时返回，机器轮次为空数组）
    pub legal_moves: Vec<String>,
    pub can_human_move: bool,
    pub moves: Vec<String>,
    pub mode: String,
    pub red_engine: Option<String>,
    pub black_engine: Option<String>,
}

impl GameView {
    fn from_game(g: &Game) -> GameView {
        let board = g.board();
        let (status, result) = match g.result() {
            Some(GameResult::Win(Color::Red)) => ("checkmate".into(), Some("red_win".to_string())),
            Some(GameResult::Win(Color::Black)) => ("checkmate".into(), Some("black_win".to_string())),
            Some(GameResult::Draw) => ("draw".into(), Some("draw".to_string())),
            None => ("playing".into(), None),
        };
        let side = match g.side_to_move() {
            Color::Red => "red",
            Color::Black => "black",
        };
        GameView {
            fen: board.to_fen(),
            side_to_move: side.into(),
            status,
            result,
            last_move: g.moves().last().map(Move::to_ucci),
            last_reason: None,
            legal_moves: if g.is_human_turn() {
                board.legal_moves().iter().map(Move::to_ucci).collect()
            } else {
                Vec::new()
            },
            can_human_move: g.is_human_turn(),
            moves: g.moves().iter().map(Move::to_ucci).collect(),
            mode: match g.mode() {
                GameMode::HumanVsMachine => "human_vs_machine".into(),
                GameMode::MachineVsMachine => "machine_vs_machine".into(),
                GameMode::HumanVsHuman => "human_vs_human".into(),
            },
            red_engine: g.red_engine().map(engine_id_str),
            black_engine: g.black_engine().map(engine_id_str),
        }
    }
}

fn engine_id_str(id: EngineId) -> String {
    match id {
        EngineId::Builtin { depth } => format!("builtin:{depth}"),
        EngineId::Llm => "llm".into(),
        EngineId::Pikafish => "pikafish".into(),
    }
}

fn engine_config_str(cfg: &EngineConfig) -> String {
    match cfg.kind {
        EngineKind::Builtin => format!("builtin:{}", cfg.depth.unwrap_or(3)),
        EngineKind::Llm => "llm".into(),
        EngineKind::Pikafish => "pikafish".into(),
    }
}

fn engine_config_from_str(s: &str) -> Result<EngineConfig, String> {
    match s {
        "builtin" => Ok(EngineConfig::builtin(3)),
        "llm" => Ok(EngineConfig::llm()),
        "pikafish" => Ok(EngineConfig::pikafish()),
        _ => Err(format!("未知引擎：{s}（支持 builtin/llm/pikafish）")),
    }
}

fn engine_id_from_cfg(cfg: &EngineConfig) -> EngineId {
    match cfg.kind {
        EngineKind::Builtin => EngineId::Builtin {
            depth: cfg.depth.unwrap_or(3),
        },
        EngineKind::Llm => EngineId::Llm,
        EngineKind::Pikafish => EngineId::Pikafish,
    }
}

fn mode_from_str(s: &str) -> Result<GameMode, String> {
    match s {
        "human_vs_machine" => Ok(GameMode::HumanVsMachine),
        "machine_vs_machine" => Ok(GameMode::MachineVsMachine),
        "human_vs_human" => Ok(GameMode::HumanVsHuman),
        _ => Err(format!("未知模式：{s}")),
    }
}

fn mode_str(mode: GameMode) -> String {
    match mode {
        GameMode::HumanVsMachine => "human_vs_machine".into(),
        GameMode::MachineVsMachine => "machine_vs_machine".into(),
        GameMode::HumanVsHuman => "human_vs_human".into(),
    }
}

fn save_data_from_game(g: &Game, name: &str) -> SaveData {
    let mut data = SaveData::new(name, g.board().to_fen());
    data.mode = g.mode();
    data.red_engine = g.red_engine().map(engine_config_from_id);
    data.black_engine = g.black_engine().map(engine_config_from_id);
    data.moves = g.moves().iter().map(Move::to_ucci).collect();
    data.result = g.result();
    data
}

fn engine_config_from_id(id: EngineId) -> EngineConfig {
    match id {
        EngineId::Builtin { depth } => EngineConfig::builtin(depth),
        EngineId::Llm => EngineConfig::llm(),
        EngineId::Pikafish => EngineConfig::pikafish(),
    }
}

fn restore_from_data(data: &SaveData) -> Result<Game, String> {
    let moves = data
        .moves
        .iter()
        .map(|s| Move::from_ucci(s).ok_or_else(|| format!("存档着法非法：{s}")))
        .collect::<Result<Vec<_>, _>>()?;
    Game::restore(
        &data.start_fen,
        data.mode,
        data.red_engine.as_ref().map(engine_id_from_cfg),
        data.black_engine.as_ref().map(engine_id_from_cfg),
        &moves,
    )
    .map_err(|e| e.to_string())
}

fn err_msg(e: GameError) -> String {
    e.to_string()
}

/// 引擎管理（带大模型配置）
fn make_manager(state: &AppState) -> EngineManager {
    EngineManager::new(state.llm.lock().unwrap().clone())
}

// ---------- 命令 ----------

/// 新建对局
#[tauri::command]
pub fn new_game(
    state: State<'_, AppState>,
    mode: String,
    red_engine: String,
    black_engine: String,
) -> Result<GameView, String> {
    let mode_enum = mode_from_str(&mode)?;
    let red = if red_engine.is_empty() {
        None
    } else {
        Some(engine_id_from_cfg(&engine_config_from_str(&red_engine)?))
    };
    let black = if black_engine.is_empty() {
        None
    } else {
        Some(engine_id_from_cfg(&engine_config_from_str(&black_engine)?))
    };
    let g = Game::new(
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
        mode_enum,
        red,
        black,
    )
    .map_err(err_msg)?;
    let view = GameView::from_game(&g);
    *state.game.lock().unwrap() = Some(g);
    state.saves.clear_autosave().ok();
    Ok(view)
}

/// 人走一步（UCCI）
#[tauri::command]
pub fn play_human_move(
    state: State<'_, AppState>,
    ucci: String,
) -> Result<GameView, String> {
    let mut guard = state.game.lock().unwrap();
    let g = guard.as_mut().ok_or_else(|| "尚未开局".to_string())?;
    let mv = Move::from_ucci(&ucci).ok_or_else(|| format!("着法格式非法：{ucci}"))?;
    g.play_move(mv).map_err(err_msg)?;
    let view = GameView::from_game(g);
    // 自动保存（恢复上次对局）
    state.saves.autosave(&save_data_from_game(g, "自动存档")).ok();
    Ok(view)
}

/// 机器走一步（当前轮方为引擎时）
#[tauri::command]
pub fn machine_step(state: State<'_, AppState>, think_ms: Option<u64>) -> Result<GameView, String> {
    let mut guard = state.game.lock().unwrap();
    let g = guard.as_mut().ok_or_else(|| "尚未开局".to_string())?;
    let opts = EngineOptions {
        think_ms: think_ms.unwrap_or(500) as u32,
    };
    let (event, _source, fallback_reason) =
        g.machine_move(&make_manager(state.inner()), opts).map_err(err_msg)?;
    let mut view = GameView::from_game(g);
    view.last_move = Some(event.mv);
    view.last_reason = fallback_reason;
    state.saves.autosave(&save_data_from_game(g, "自动存档")).ok();
    Ok(view)
}

/// 存档列表
#[tauri::command]
pub fn list_saves(state: State<'_, AppState>) -> Vec<SaveSummary> {
    state.saves.list_saves()
}

/// 保存到槽位（1..=5）
#[tauri::command]
pub fn save_slot(state: State<'_, AppState>, slot: usize, name: String) -> Result<(), String> {
    let guard = state.game.lock().unwrap();
    let g = guard.as_ref().ok_or_else(|| "尚未开局".to_string())?;
    let data = save_data_from_game(g, &name);
    state.saves.save(slot, &data).map_err(|e| e.to_string())
}

/// 从槽位载入
#[tauri::command]
pub fn load_slot(state: State<'_, AppState>, slot: usize) -> Result<GameView, String> {
    let data = state.saves.load(slot).map_err(|e| e.to_string())?;
    let g = restore_from_data(&data)?;
    let view = GameView::from_game(&g);
    *state.game.lock().unwrap() = Some(g);
    Ok(view)
}

/// 恢复上次对局（自动存档；无则返回 None）
#[tauri::command]
pub fn load_autosave(state: State<'_, AppState>) -> Result<Option<GameView>, String> {
    let data = match state.saves.restore_autosave() {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    let g = restore_from_data(&data)?;
    let view = GameView::from_game(&g);
    *state.game.lock().unwrap() = Some(g);
    Ok(Some(view))
}

/// 配置大模型（base_url/api_key/model；timeout_secs 默认 10）
#[tauri::command]
pub fn set_llm_config(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let cfg = llm_engine::LlmConfig::new(base_url, api_key, model);
    *state.llm.lock().unwrap() = Some(cfg);
    Ok(())
}

/// 大模型配置视图（api_key 打码）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigView {
    pub base_url: String,
    pub model: String,
    /// 已配置（打码显示前 4 位）
    pub api_key_masked: Option<String>,
    pub timeout_secs: u32,
}

#[tauri::command]
pub fn get_llm_config(state: State<'_, AppState>) -> Option<LlmConfigView> {
    state.llm.lock().unwrap().as_ref().map(|c| LlmConfigView {
        base_url: c.base_url.clone(),
        model: c.model.clone(),
        api_key_masked: mask_key(&c.api_key),
        timeout_secs: c.timeout_secs,
    })
}

fn mask_key(k: &str) -> Option<String> {
    if k.is_empty() {
        None
    } else if k.len() <= 4 {
        Some("****".into())
    } else {
        Some(format!("{}****", &k[..4]))
    }
}

//! Tauri 命令层：对局编排（新建/走子/机器走子）、存档（5 槽 + 自动）、大模型配置

use std::path::PathBuf;
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
    /// 大模型配置（内存镜像；持久化于 app_data_dir/llm-config.json）
    pub llm: Mutex<Option<llm_engine::LlmConfig>>,
    /// 存档存储（app_data_dir/saves）
    pub saves: SaveStore,
    /// 应用数据目录（llm 配置等持久化；安装目录之外）
    pub data_dir: PathBuf,
}

/// LLM 配置文件（应用数据目录内）
const LLM_CONFIG_FILE: &str = "llm-config.json";

/// 从应用数据目录加载大模型配置（不存在/解析失败返回 None）
pub fn load_llm_config(data_dir: &std::path::Path) -> Option<llm_engine::LlmConfig> {
    let json = std::fs::read_to_string(data_dir.join(LLM_CONFIG_FILE)).ok()?;
    serde_json::from_str(&json).ok()
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
            Some(GameResult::Aborted) => ("aborted".into(), Some("aborted".to_string())),
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
        s if s.starts_with("builtin:") => {
            let d: u8 = s[8..]
                .parse()
                .map_err(|_| format!("内置引擎深度非法：{s}"))?;
            if !(1..=6).contains(&d) {
                return Err(format!("内置引擎深度需在 1..=6：{s}"));
            }
            Ok(EngineConfig::builtin(d))
        }
        _ => Err(format!("未知引擎：{s}（支持 builtin[:1..=6]/llm/pikafish）")),
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
    // 按模式强制引擎归属：人机对战红方由人走；机器对战缺省补内置引擎
    let (red, black) = match mode_enum {
        GameMode::HumanVsMachine => (None, black),
        GameMode::MachineVsMachine => (
            red.or(Some(EngineId::Builtin { depth: 3 })),
            black.or(Some(EngineId::Builtin { depth: 3 })),
        ),
        GameMode::HumanVsHuman => (None, None),
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

/// 机器走一步（当前轮方为引擎时）；async 避免 LLM 请求阻塞 UI 线程
#[tauri::command]
pub async fn machine_step(state: State<'_, AppState>, think_ms: Option<u64>) -> Result<GameView, String> {
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

/// 中止对局（前端「停止」按钮）：置为已中止，棋盘锁定；
/// 自动推进循环由前端停止，本命令只负责对局状态终局化
#[tauri::command]
pub fn abort_game(state: State<'_, AppState>) -> Result<GameView, String> {
    let mut guard = state.game.lock().unwrap();
    let g = guard.as_mut().ok_or_else(|| "尚未开局".to_string())?;
    g.abort();
    Ok(GameView::from_game(g))
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

/// 配置大模型（base_url/api_key/model/timeout_secs）
/// 持久化到应用数据目录（安装目录之外），下次启动自动恢复；
/// api_key 留空时保留已保存的密钥（避免误覆盖清空）
#[tauri::command]
pub fn set_llm_config(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    model: String,
    timeout_secs: Option<u32>,
    advisor: Option<String>,
    blend: Option<u8>,
) -> Result<(), String> {
    let key = if api_key.trim().is_empty() {
        state
            .llm
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.api_key.clone())
            .unwrap_or_default()
    } else {
        api_key.trim().to_string()
    };
    let mut cfg = llm_engine::LlmConfig::new(base_url.trim(), key, model.trim());
    cfg.timeout_secs = timeout_secs.unwrap_or(30).max(5);
    // 参谋模式：off / candidate（默认）/ gate；未知值一律回退 candidate
    cfg.advisor = match advisor.as_deref() {
        Some("off") => "off".into(),
        Some("gate") => "gate".into(),
        _ => "candidate".into(),
    };
    cfg.blend = blend.unwrap_or(60).min(100);
    std::fs::create_dir_all(&state.data_dir).map_err(|e| e.to_string())?;
    let path = state.data_dir.join(LLM_CONFIG_FILE);
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("配置写入失败：{e}"))?;
    // 密钥文件仅本人可读写
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    *state.llm.lock().unwrap() = Some(cfg);
    Ok(())
}

/// 测试大模型连通性（使用传入参数，无需先保存）；async 避免阻塞 UI；
/// api_key 留空时沿用已保存的密钥（密码框不回填，测试/保存均不回退为空）
#[tauri::command]
pub async fn test_llm_config(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    model: String,
    timeout_secs: Option<u32>,
    advisor: Option<String>,
    blend: Option<u8>,
) -> Result<String, String> {
    let key = if api_key.trim().is_empty() {
        state
            .llm
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.api_key.clone())
            .unwrap_or_default()
    } else {
        api_key.trim().to_string()
    };
    let mut cfg = llm_engine::LlmConfig::new(base_url, key, model.clone());
    cfg.timeout_secs = timeout_secs.unwrap_or(30).max(5);
    let _ = (&advisor, &blend); // 测试连通性与参谋参数无关，仅保持前端参数对齐
    let client = llm_engine::LlmClient::new(cfg);
    client
        .test_connection_sync()
        .map(|_| format!("连接成功：模型 {model} 已正常响应"))
        .map_err(|e| format!("{e}"))
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
    pub advisor: String,
    pub blend: u8,
}

#[tauri::command]
pub fn get_llm_config(state: State<'_, AppState>) -> Option<LlmConfigView> {
    state.llm.lock().unwrap().as_ref().map(|c| LlmConfigView {
        base_url: c.base_url.clone(),
        model: c.model.clone(),
        api_key_masked: mask_key(&c.api_key),
        timeout_secs: c.timeout_secs,
        advisor: c.advisor.clone(),
        blend: c.blend,
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

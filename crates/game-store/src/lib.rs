//! game-store: 持久化（槽位存档 + 自动存档 + 校验重放）
//!
//! 文件布局（root_dir 由调用方传入，如 tauri 的 app_data_dir()）：
//! - save_1.json .. save_5.json：手动槽位
//! - autosave.json：每步落子/退出时写入（崩溃恢复）
//!
//! 写入采用"临时文件 + rename"原子替换，防止中途崩溃损坏存档。

use std::path::{Path, PathBuf};

use game_core::model::{EngineConfig, GameMode, GameResult};
use serde::{Deserialize, Serialize};
use xiangqi_core::board::Board;
use xiangqi_core::types::{Color, Move};

pub const SLOT_COUNT: usize = 5;
const AUTOSAVE_FILE: &str = "autosave.json";

/// 存档数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub name: String,
    pub mode: GameMode,
    /// 红方引擎（人机对战时为人的一方可省略）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub red_engine: Option<EngineConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black_engine: Option<EngineConfig>,
    /// 起始局面 FEN
    pub start_fen: String,
    /// 已走着法（UCCI）
    #[serde(default)]
    pub moves: Vec<String>,
    #[serde(default)]
    pub result: Option<GameResult>,
    /// 创建/更新时间（Unix 秒）
    pub created_at: u64,
    pub updated_at: u64,
}

impl SaveData {
    pub fn new(name: impl Into<String>, start_fen: impl Into<String>) -> Self {
        let now = now_secs();
        SaveData {
            version: 1,
            name: name.into(),
            mode: GameMode::HumanVsMachine,
            red_engine: None,
            black_engine: None,
            start_fen: start_fen.into(),
            moves: Vec::new(),
            result: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// 存档列表摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveSummary {
    pub slot: usize,
    pub name: String,
    pub updated_at: u64,
    /// 摘要状态：进行中/红胜/黑胜/和棋
    pub status: String,
    pub move_count: usize,
}

/// 持久化存储
#[derive(Debug, Clone)]
pub struct SaveStore {
    root: PathBuf,
}

impl SaveStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root).ok();
        SaveStore { root }
    }

    fn slot_path(&self, slot: usize) -> Result<PathBuf, PersistError> {
        if !(1..=SLOT_COUNT).contains(&slot) {
            return Err(PersistError::SlotOutOfRange(slot));
        }
        Ok(self.root.join(format!("save_{slot}.json")))
    }

    fn autosave_path(&self) -> PathBuf {
        self.root.join(AUTOSAVE_FILE)
    }

    /// 保存到槽位（原子写入）
    pub fn save(&self, slot: usize, data: &SaveData) -> Result<(), PersistError> {
        let path = self.slot_path(slot)?;
        atomic_write(&path, data)
    }

    /// 读取槽位（损坏/不存在均返回错误，不 panic）
    pub fn load(&self, slot: usize) -> Result<SaveData, PersistError> {
        let path = self.slot_path(slot)?;
        if !path.exists() {
            return Err(PersistError::NotFound(path));
        }
        read_json(&path)
    }

    pub fn delete_save(&self, slot: usize) -> Result<(), PersistError> {
        let path = self.slot_path(slot)?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(PersistError::Io)?;
        }
        Ok(())
    }

    /// 槽位列表（存在且可解析的槽）
    pub fn list_saves(&self) -> Vec<SaveSummary> {
        (1..=SLOT_COUNT)
            .filter_map(|slot| {
                let data = self.load(slot).ok()?;
                let status = summarize(&data);
                Some(SaveSummary {
                    slot,
                    name: data.name,
                    updated_at: data.updated_at,
                    status,
                    move_count: data.moves.len(),
                })
            })
            .collect()
    }

    pub fn has_autosave(&self) -> bool {
        self.autosave_path().exists()
    }

    /// 自动存档（覆盖写）
    pub fn autosave(&self, data: &SaveData) -> Result<(), PersistError> {
        atomic_write(&self.autosave_path(), data)
    }

    pub fn restore_autosave(&self) -> Result<SaveData, PersistError> {
        let path = self.autosave_path();
        if !path.exists() {
            return Err(PersistError::NotFound(path));
        }
        read_json(&path)
    }

    pub fn clear_autosave(&self) -> Result<(), PersistError> {
        let path = self.autosave_path();
        if path.exists() {
            std::fs::remove_file(&path).map_err(PersistError::Io)?;
        }
        Ok(())
    }
}

/// 校验并重放存档：FEN 可解析、每步为合法着（相对前局面）、轮走方连续
/// 返回重放后的最终局面与状态
pub fn validate_and_replay(
    data: &SaveData,
) -> Result<(Board, xiangqi_core::types::GameStatus), PersistError> {
    let mut board = Board::parse_fen(&data.start_fen).map_err(PersistError::InvalidSave)?;
    for (i, u) in data.moves.iter().enumerate() {
        let mv = Move::from_ucci(u).ok_or_else(|| PersistError::IllegalMove(i, u.clone()))?;
        if !board.legal_moves().contains(&mv) {
            return Err(PersistError::IllegalMove(i, u.clone()));
        }
        board = board.make_move(mv);
    }
    let status = board.game_status();
    Ok((board, status))
}

fn summarize(data: &SaveData) -> String {
    match data.result {
        Some(GameResult::Win(Color::Red)) => "红胜".to_string(),
        Some(GameResult::Win(_)) => "黑胜".to_string(),
        Some(GameResult::Draw) => "和棋".to_string(),
        None => "进行中".to_string(),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 原子写：临时文件 + rename
fn atomic_write<T: Serialize>(path: &Path, value: &T) -> Result<(), PersistError> {
    let json = serde_json::to_string_pretty(value).map_err(PersistError::Serialize)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(PersistError::Io)?;
    std::fs::rename(&tmp, path).map_err(PersistError::Io)?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, PersistError> {
    let s = std::fs::read_to_string(path).map_err(PersistError::Io)?;
    serde_json::from_str(&s).map_err(PersistError::Deserialize)
}

/// 持久化错误
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("槽位越界：{0}（有效 1..={SLOT_COUNT}）")]
    SlotOutOfRange(usize),
    #[error("存档不存在：{0}")]
    NotFound(PathBuf),
    #[error("I/O 错误：{0}")]
    Io(std::io::Error),
    #[error("序列化错误：{0}")]
    Serialize(serde_json::Error),
    #[error("JSON 损坏：{0}")]
    Deserialize(serde_json::Error),
    #[error("存档无效：{0}")]
    InvalidSave(String),
    #[error("第 {0} 步非法着：{1}")]
    IllegalMove(usize, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_range() {
        let s = SaveStore::new(std::env::temp_dir().join("xiq-store-test-range"));
        assert!(s.slot_path(0).is_err());
        assert!(s.slot_path(6).is_err());
        assert!(s.slot_path(1).is_ok());
        assert!(s.slot_path(5).is_ok());
    }
}

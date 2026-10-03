//! 中国象棋 App（Tauri 2）——阶段一：人机/机器对战 + 持久化 + LLM 引擎降级

mod commands;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("无法获取应用数据目录");
            app.manage(AppState {
                game: std::sync::Mutex::new(None),
                llm: std::sync::Mutex::new(None),
                saves: game_store::SaveStore::new(data_dir.join("saves")),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::new_game,
            commands::play_human_move,
            commands::machine_step,
            commands::list_saves,
            commands::save_slot,
            commands::load_slot,
            commands::load_autosave,
            commands::set_llm_config,
            commands::get_llm_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

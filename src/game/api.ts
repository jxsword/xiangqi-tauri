// Tauri 命令封装

import { invoke } from "@tauri-apps/api/core";
import type { GameView, LlmConfigView, SaveSummary } from "./types";

export const api = {
  newGame(mode: string, redEngine: string, blackEngine: string): Promise<GameView> {
    return invoke<GameView>("new_game", { mode, redEngine, blackEngine });
  },
  playHumanMove(ucci: string): Promise<GameView> {
    return invoke<GameView>("play_human_move", { ucci });
  },
  machineStep(thinkMs?: number): Promise<GameView> {
    return invoke<GameView>("machine_step", { thinkMs });
  },
  abortGame(): Promise<GameView> {
    return invoke<GameView>("abort_game");
  },
  listSaves(): Promise<SaveSummary[]> {
    return invoke<SaveSummary[]>("list_saves");
  },
  saveSlot(slot: number, name: string): Promise<void> {
    return invoke<void>("save_slot", { slot, name });
  },
  loadSlot(slot: number): Promise<GameView> {
    return invoke<GameView>("load_slot", { slot });
  },
  loadAutosave(): Promise<GameView | null> {
    return invoke<GameView | null>("load_autosave");
  },
  setLlmConfig(baseUrl: string, apiKey: string, model: string, timeoutSecs: number, advisor: string, blend: number): Promise<void> {
    return invoke<void>("set_llm_config", { baseUrl, apiKey, model, timeoutSecs, advisor, blend });
  },
  testLlmConfig(baseUrl: string, apiKey: string, model: string, timeoutSecs: number, advisor: string, blend: number): Promise<string> {
    return invoke<string>("test_llm_config", { baseUrl, apiKey, model, timeoutSecs, advisor, blend });
  },
  getLlmConfig(): Promise<LlmConfigView | null> {
    return invoke<LlmConfigView | null>("get_llm_config");
  },
};

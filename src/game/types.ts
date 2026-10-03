// 与 Rust 命令层对应的前端类型（serde camelCase）

export interface GameView {
  fen: string;
  /** "red" | "black" */
  sideToMove: "red" | "black";
  /** playing | checkmate | stalemate | draw */
  status: "playing" | "checkmate" | "stalemate" | "draw";
  /** red_win | black_win | draw | null */
  result: "red_win" | "black_win" | "draw" | null;
  lastMove: string | null;
  /** 大模型降级原因（null=正常） */
  lastReason: string | null;
  /** 当前轮方全部合法着（仅人走时非空） */
  legalMoves: string[];
  canHumanMove: boolean;
  moves: string[];
  mode: "human_vs_machine" | "machine_vs_machine" | "human_vs_human";
  redEngine: string | null;
  blackEngine: string | null;
}

export interface SaveSummary {
  slot: number;
  name: string;
  updatedAt: number;
  /** 进行中/红胜/黑胜/和棋 */
  status: string;
  moveCount: number;
}

export interface LlmConfigView {
  baseUrl: string;
  model: string;
  /** 已配置（打码） */
  apiKeyMasked: string | null;
  timeoutSecs: number;
}

export interface EngineOption {
  /** builtin | llm | pikafish */
  value: string;
  label: string;
}

export interface ModeOption {
  value: "human_vs_machine" | "machine_vs_machine" | "human_vs_human";
  label: string;
}

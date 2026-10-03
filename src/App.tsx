import { useCallback, useEffect, useRef, useState } from "react";
import BoardCanvas from "./board/BoardCanvas";
import { api } from "./game/api";
import type { GameView, LlmConfigView, SaveSummary } from "./game/types";

const ENGINES = [
  { value: "builtin", label: "内置引擎" },
  { value: "llm", label: "大模型引擎（失败自动降级内置）" },
  { value: "pikafish", label: "皮卡鱼（桌面端）" },
];

const MODES = [
  { value: "human_vs_machine", label: "人机对战" },
  { value: "machine_vs_machine", label: "机器对战" },
];

const SPEEDS = [
  { value: "fast", label: "快速（深度 2）", depth: 2, thinkMs: 200 },
  { value: "normal", label: "标准（深度 3）", depth: 3, thinkMs: 800 },
  { value: "deep", label: "深度思考（深度 6）", depth: 6, thinkMs: 5000 },
];

const RESULT_TEXT: Record<string, string> = {
  red_win: "红方胜",
  black_win: "黑方胜",
  draw: "和棋",
};

export default function App() {
  const [view, setView] = useState<GameView | null>(null);
  const [mode, setMode] = useState("human_vs_machine");
  const [redEngine, setRedEngine] = useState("builtin");
  const [blackEngine, setBlackEngine] = useState("llm");
  const [speed, setSpeed] = useState("normal");
  const speedRef = useRef<{ depth: number; thinkMs: number }>(SPEEDS[1]);
  useEffect(() => {
    speedRef.current = SPEEDS.find((s) => s.value === speed) ?? SPEEDS[1];
  }, [speed]);
  const [saves, setSaves] = useState<SaveSummary[]>([]);
  const [llmCfg, setLlmCfg] = useState<LlmConfigView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  const showErr = (e: unknown) => setErr(e instanceof Error ? e.message : String(e));

  const refreshSaves = useCallback(() => {
    api.listSaves().then(setSaves).catch(showErr);
  }, []);

  // 机器轮次自动推进（人机：黑方机器走完即停；机器对战：直至终局）
  const driveIfMachine = useCallback(async (v: GameView) => {
    if (!v || v.result || v.canHumanMove) return;
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      for (let i = 0; i < 200; i++) {
        const next = await api.machineStep(speedRef.current.thinkMs);
        setView(next);
        if (next.result || next.canHumanMove) break;
      }
    } catch (e) {
      showErr(e);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, []);

  const applyView = useCallback(
    (v: GameView) => {
      setView(v);
      void driveIfMachine(v);
    },
    [driveIfMachine]
  );

  // 启动：自动恢复上次对局 + 载入配置/存档列表
  useEffect(() => {
    api
      .loadAutosave()
      .then((v) => {
        if (v) {
          setMode(v.mode);
          setRedEngine(v.redEngine?.split(":")[0] ?? "builtin");
          setBlackEngine(v.blackEngine?.split(":")[0] ?? "llm");
          applyView(v);
        }
      })
      .catch(showErr);
    api.getLlmConfig().then(setLlmCfg).catch(showErr);
    refreshSaves();
  }, [applyView, refreshSaves]);

  const engineArg = (e: string) =>
    e === "builtin" ? `builtin:${speedRef.current.depth}` : e;

  const handleNewGame = () => {
    setBusy(true);
    api
      .newGame(mode, engineArg(redEngine), engineArg(blackEngine))
      .then(applyView)
      .catch(showErr)
      .finally(() => setBusy(false));
  };

  const handleHumanMove = (ucci: string) => {
    if (busyRef.current) return;
    setBusy(true);
    api
      .playHumanMove(ucci)
      .then(applyView)
      .catch(showErr)
      .finally(() => setBusy(false));
  };

  const handleSave = (slot: number) => {
    api
      .saveSlot(slot, `第 ${slot} 局`)
      .then(refreshSaves)
      .catch(showErr);
  };

  const handleLoad = (slot: number) => {
    api
      .loadSlot(slot)
      .then(applyView)
      .catch(showErr);
  };

  const handleSetLlm = async (baseUrl: string, apiKey: string, model: string, timeoutSecs: number) => {
    await api.setLlmConfig(baseUrl.trim(), apiKey.trim(), model.trim(), timeoutSecs);
    setLlmCfg(await api.getLlmConfig());
  };

  const statusText = (v: GameView) => {
    if (v.result) return `终局 · ${RESULT_TEXT[v.result] ?? v.result}`;
    const turn = v.sideToMove === "red" ? "红方" : "黑方";
    if (v.canHumanMove) return `轮到您（${turn}）走棋`;
    return `${turn} 思考中…`;
  };

  return (
    <div
      style={{
        fontFamily: "'Noto Serif SC','KaiTi','STKaiti',sans-serif",
        display: "flex",
        flexWrap: "wrap",
        gap: 20,
        padding: 16,
        background: "#2b1d10",
        minHeight: "100%",
        boxSizing: "border-box",
        color: "#f0e6d2",
      }}
    >
      <div style={{ flex: "0 0 auto", maxWidth: 720 }}>
        {view ? (
          <BoardCanvas view={view} onMove={handleHumanMove} />
        ) : (
          <div
            style={{
              width: 720,
              height: 480,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              border: "1px dashed #7a5a3a",
              borderRadius: 8,
            }}
          >
            尚未开局，请在右侧设置后点击「新建对局」
          </div>
        )}
      </div>

      <div style={{ flex: "1 1 300px", minWidth: 300 }}>
        <h2 style={{ marginTop: 0, fontSize: 20 }}>对局设置</h2>
        <label>
          模式
          <select
            value={mode}
            onChange={(e) => setMode(e.target.value)}
            disabled={busy}
            style={inputStyle}
          >
            {MODES.map((m) => (
              <option key={m.value} value={m.value} style={optionStyle}>
                {m.label}
              </option>
            ))}
          </select>
        </label>
        {mode === "machine_vs_machine" ? (
          <>
            <label>
              红方引擎
              <select value={redEngine} onChange={(e) => setRedEngine(e.target.value)} style={inputStyle}>
                {ENGINES.map((m) => (
                  <option key={m.value} value={m.value} style={optionStyle}>
                    {m.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              黑方引擎
              <select value={blackEngine} onChange={(e) => setBlackEngine(e.target.value)} style={inputStyle}>
                {ENGINES.map((m) => (
                  <option key={m.value} value={m.value} style={optionStyle}>
                    {m.label}
                  </option>
                ))}
              </select>
            </label>
          </>
        ) : (
          <label>
            对手引擎
            <select value={blackEngine} onChange={(e) => setBlackEngine(e.target.value)} style={inputStyle}>
              {ENGINES.map((m) => (
                <option key={m.value} value={m.value} style={optionStyle}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>
        )}

        <label>
          引擎速度（内置引擎）
          <select value={speed} onChange={(e) => setSpeed(e.target.value)} disabled={busy} style={inputStyle}>
            {SPEEDS.map((s) => (
              <option key={s.value} value={s.value} style={optionStyle}>
                {s.label}
              </option>
            ))}
          </select>
        </label>

        <button onClick={handleNewGame} disabled={busy} style={btnStyle}>
          {view ? "重新开局" : "新建对局"}
        </button>

        {view && (
          <div style={{ marginTop: 16, padding: "10px 12px", background: "#3a2a1a", borderRadius: 8 }}>
            <div>{statusText(view)}</div>
            <div style={{ opacity: 0.8, fontSize: 13 }}>
              已走 {view.moves.length} 步 · 最近一着：{view.lastMove ?? "—"}
            </div>
            {view.lastReason && (
              <div style={{ color: "#ffb35c", fontSize: 13, marginTop: 4 }}>⚠ {view.lastReason}</div>
            )}
          </div>
        )}

        {err && <div style={{ color: "#ff8a80", fontSize: 13, marginTop: 8 }}>错误：{err}</div>}

        <LlmPanel cfg={llmCfg} onSave={handleSetLlm} />

        <h2 style={{ fontSize: 20 }}>存档（5 槽位 + 自动恢复）</h2>
        <button onClick={refreshSaves} style={btnStyle}>
          刷新存档
        </button>
        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 14 }}>
          <thead>
            <tr>
              <th style={thStyle}>槽位</th>
              <th style={thStyle}>名称</th>
              <th style={thStyle}>状态</th>
              <th style={thStyle}>步数</th>
              <th style={thStyle}>操作</th>
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: 5 }, (_, i) => i + 1).map((slot) => {
              const s = saves.find((x) => x.slot === slot);
              return (
                <tr key={slot} style={{ borderTop: "1px solid #4a3a2a" }}>
                  <td style={tdStyle}>{slot}</td>
                  <td style={tdStyle}>{s?.name ?? "空"}</td>
                  <td style={tdStyle}>{s?.status ?? "—"}</td>
                  <td style={tdStyle}>{s?.moveCount ?? "—"}</td>
                  <td style={tdStyle}>
                    <button onClick={() => handleSave(slot)} disabled={!view} style={miniBtn}>
                      保存
                    </button>{" "}
                    {s && (
                      <button onClick={() => handleLoad(slot)} style={miniBtn}>
                        载入
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function LlmPanel({
  cfg,
  onSave,
}: {
  cfg: LlmConfigView | null;
  onSave: (baseUrl: string, apiKey: string, model: string, timeoutSecs: number) => Promise<void>;
}) {
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [dirty, setDirty] = useState(false);
  const [timeoutSecs, setTimeoutSecs] = useState(30);
  const [notice, setNotice] = useState<{ kind: "ok" | "err"; text: string } | null>(null);

  const showNotice = (kind: "ok" | "err", text: string) => {
    setNotice({ kind, text });
    window.setTimeout(() => setNotice(null), 6000);
  };

  const handleSave = async () => {
    try {
      await onSave(baseUrl, apiKey, model, timeoutSecs);
      showNotice("ok", "配置已保存（应用数据目录），下次启动自动恢复");
    } catch (e) {
      showNotice("err", `保存失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleTest = async () => {
    try {
      const msg = await api.testLlmConfig(baseUrl.trim(), apiKey.trim(), model.trim(), timeoutSecs);
      showNotice("ok", msg);
    } catch (e) {
      showNotice("err", `连接失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  useEffect(() => {
    if (cfg && !dirty) {
      setBaseUrl(cfg.baseUrl);
      setModel(cfg.model);
      setTimeoutSecs(cfg.timeoutSecs);
    }
  }, [cfg, dirty]);

  return (
    <div style={{ marginTop: 16 }}>
      <h2 style={{ fontSize: 20 }}>大模型设置</h2>
      <label>
        API 地址（base_url）
        <input
          value={baseUrl}
          onChange={(e) => { setBaseUrl(e.target.value); setDirty(true); }}
          placeholder="https://api.openai.com/v1"
          style={inputStyle}
        />
      </label>
      <label>
        API Key（{cfg?.apiKeyMasked ?? "未配置"}）
        <input
          value={apiKey}
          onChange={(e) => { setApiKey(e.target.value); setDirty(true); }}
          type="password"
          placeholder={cfg?.apiKeyMasked ? "已保存，无需重复输入（如需更换再填写）" : "sk-..."}
          style={inputStyle}
        />
      </label>
      <label>
        模型
        <input
          value={model}
          onChange={(e) => { setModel(e.target.value); setDirty(true); }}
          placeholder="gpt-4o-mini"
          style={inputStyle}
        />
      </label>
      <label>
        超时秒数（大模型响应上限）
        <input
          value={timeoutSecs}
          onChange={(e) => {
            setTimeoutSecs(Number(e.target.value) || 30);
            setDirty(true);
          }}
          type="number"
          min={5}
          max={120}
          style={inputStyle}
        />
      </label>
      <button onClick={handleSave} style={btnStyle}>
        保存配置
      </button>{" "}
      <button onClick={handleTest} style={miniBtn}>
        测试连接
      </button>
      {notice && (
        <div
          style={{
            color: notice.kind === "ok" ? "#7fd18c" : "#ff8a80",
            fontSize: 13,
            marginTop: 6,
          }}
        >
          {notice.text}
        </div>
      )}
      <div style={{ fontSize: 12, opacity: 0.7, marginTop: 4 }}>
        超时 10 秒；请求失败/超时/非法着将自动降级为内置引擎
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  boxSizing: "border-box",
  margin: "4px 0 10px",
  padding: "6px 8px",
  borderRadius: 6,
  border: "1px solid #7a5a3a",
  background: "#1f150c",
  color: "#f0e6d2",
  colorScheme: "dark",
  fontSize: 14,
};

const optionStyle: React.CSSProperties = {
  background: "#1f150c",
  color: "#f0e6d2",
};

const btnStyle: React.CSSProperties = {
  padding: "8px 16px",
  borderRadius: 6,
  border: "none",
  background: "#b03a2e",
  color: "#fff",
  fontSize: 15,
  cursor: "pointer",
  margin: "4px 4px 4px 0",
};

const miniBtn: React.CSSProperties = {
  padding: "3px 8px",
  borderRadius: 4,
  border: "1px solid #7a5a3a",
  background: "transparent",
  color: "#f0e6d2",
  cursor: "pointer",
  fontSize: 13,
};

const thStyle: React.CSSProperties = { textAlign: "left", padding: "4px 6px" };
const tdStyle: React.CSSProperties = { padding: "6px", textAlign: "left" };

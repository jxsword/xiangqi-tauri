import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export default function App() {
  const [msg, setMsg] = useState("中国象棋 · 加载中…");

  useEffect(() => {
    invoke<string>("greet", { name: "玩家" }).then(setMsg).catch(console.error);
  }, []);

  return (
    <div style={{ fontFamily: "sans-serif", padding: 24, textAlign: "center" }}>
      <h1>{msg}</h1>
      <p>项目骨架已就绪（M1 起逐步实现棋盘与对局功能）</p>
    </div>
  );
}

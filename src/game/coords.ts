// 棋盘坐标纯函数：UCCI ↔ 交叉点，FEN 棋子表解析

export type Point = { file: number; rank: number };

/** 解析 UCCI（如 h2e2）为起终点。file 0..8 = a..i，rank 0=红方底线 */
export function ucciToMove(ucci: string): { from: Point; to: Point } | null {
  if (!/^[a-i][0-9][a-i][0-9]$/.test(ucci)) return null;
  const fileOf = (c: string) => c.charCodeAt(0) - "a".charCodeAt(0);
  return {
    from: { file: fileOf(ucci[0]), rank: Number(ucci[1]) },
    to: { file: fileOf(ucci[2]), rank: Number(ucci[3]) },
  };
}

/** 交叉点 → UCCI 起/终点 4 字符（与 Move::from_ucci 对应） */
export function pointToUcci(p: Point): string {
  return `${String.fromCharCode(97 + p.file)}${p.rank}`;
}

/** FEN 棋盘段 → 9×10 棋子表（显示用），item=null 空位 */
export type PieceCell = { kind: string; color: "red" | "black" } | null;

const FEN_PIECE: Record<string, { kind: string; color: "red" | "black" }> = {
  r: { kind: "车", color: "red" },
  n: { kind: "马", color: "red" },
  b: { kind: "相", color: "red" },
  a: { kind: "仕", color: "red" },
  k: { kind: "帅", color: "red" },
  c: { kind: "炮", color: "red" },
  p: { kind: "兵", color: "red" },
  R: { kind: "车", color: "black" },
  N: { kind: "马", color: "black" },
  B: { kind: "象", color: "black" },
  A: { kind: "士", color: "black" },
  K: { kind: "将", color: "black" },
  C: { kind: "炮", color: "black" },
  P: { kind: "卒", color: "black" },
};

/**
 * 解析 FEN 棋盘段 → 9×10 表。
 * FEN 第 1 行 = rank9（黑方底线），显示时第 0 行（顶部）即 rank9。
 */
export function parseFenBoard(fenBoard: string): PieceCell[][] {
  const rows = fenBoard.split("/");
  if (rows.length !== 10) throw new Error(`FEN 行数应为 10，实际 ${rows.length}`);
  const table: PieceCell[][] = [];
  for (let rank = 9; rank >= 0; rank--) {
    const row: PieceCell[] = [];
    let col = 0;
    for (const ch of rows[9 - rank]) {
      if (ch >= "1" && ch <= "9") {
        const n = Number(ch);
        for (let i = 0; i < n; i++) row.push(null);
        col += n;
      } else {
        row.push(FEN_PIECE[ch] ?? null);
        col += 1;
      }
    }
    if (col !== 9) throw new Error(`FEN 行 ${rank} 列数应为 9，实际 ${col}`);
    table.push(row);
  }
  return table;
}

/** 从完整 FEN 取棋盘段 */
export function fenBoardSegment(fen: string): string {
  return fen.split(" ")[0];
}

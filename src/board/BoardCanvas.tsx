import { useEffect, useMemo, useRef, useState } from "react";
import {
  fenBoardSegment,
  parseFenBoard,
  pointToUcci,
  ucciToMove,
} from "../game/coords";
import type { Point } from "../game/coords";
import type { GameView } from "../game/types";

const UNIT = 80; // 格间距
const MARGIN = 48; // 边距
const W = MARGIN * 2 + 8 * UNIT; // 720
const H = MARGIN * 2 + 9 * UNIT; // 816
const PIECE_R = UNIT * 0.42;

function pieceXY(p: Point) {
  return { x: MARGIN + p.file * UNIT, y: MARGIN + p.rank * UNIT };
}

interface Props {
  view: GameView;
  onMove: (ucci: string) => void;
}

/** 中国象棋棋盘：Canvas 绘制 + 点击走子（UCCI 交互） */
export default function BoardCanvas({ view, onMove }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [selected, setSelected] = useState<Point | null>(null);

  const board = useMemo(() => {
    try {
      return parseFenBoard(fenBoardSegment(view.fen));
    } catch {
      return null;
    }
  }, [view.fen]);

  // 选中棋子的合法着目标集（{file,rank}）
  const targets = useMemo(() => {
    if (!selected) return new Set<string>();
    const set = new Set<string>();
    for (const u of view.legalMoves) {
      const m = ucciToMove(u);
      if (m && m.from.file === selected.file && m.from.rank === selected.rank) {
        set.add(pointToUcci(m.to));
      }
    }
    return set;
  }, [selected, view.legalMoves]);

  useEffect(() => {
    const cv = canvasRef.current;
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    draw(ctx, board, selected, targets);
  }, [board, selected, targets, view.sideToMove]);

  const pick = (e: React.MouseEvent<HTMLCanvasElement>): Point | null => {
    const cv = canvasRef.current!;
    const rect = cv.getBoundingClientRect();
    const scaleX = cv.width / rect.width;
    const scaleY = cv.height / rect.height;
    const x = (e.clientX - rect.left) * scaleX;
    const y = (e.clientY - rect.top) * scaleY;
    const file = Math.round((x - MARGIN) / UNIT);
    const rank = Math.round((y - MARGIN) / UNIT);
    if (file < 0 || file > 8 || rank < 0 || rank > 9) return null;
    // 命中半径内才有效
    const px = MARGIN + file * UNIT;
    const py = MARGIN + rank * UNIT;
    if (Math.hypot(x - px, y - py) > PIECE_R + 4) return null;
    return { file, rank };
  };

  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!view.canHumanMove || !board) return;
    const p = pick(e);
    if (!p) return;
    const key = pointToUcci(p);
    if (targets.has(key)) {
      // 走子
      onMove(`${pointToUcci(selected!)}${key}`);
      setSelected(null);
      return;
    }
    const cell = board[p.rank][p.file];
    const myColor = view.sideToMove;
    if (cell && cell.color === myColor) {
      setSelected(p);
    } else {
      setSelected(null);
    }
  };

  return (
    <canvas
      ref={canvasRef}
      width={W}
      height={H}
      onClick={handleClick}
      style={{ width: "100%", height: "auto", touchAction: "manipulation", cursor: "pointer" }}
    />
  );
}

function draw(
  ctx: CanvasRenderingContext2D,
  board: ReturnType<typeof parseFenBoard> | null,
  selected: Point | null,
  targets: Set<string>
) {
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = "#f0d9a0";
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = "#5a3a1a";
  ctx.lineWidth = 1.6;

  // 横线 10 条
  for (let r = 0; r < 10; r++) {
    const y = MARGIN + r * UNIT;
    ctx.beginPath();
    ctx.moveTo(MARGIN, y);
    ctx.lineTo(MARGIN + 8 * UNIT, y);
    ctx.stroke();
  }
  // 竖线 9 条（河界处断开）
  for (let f = 0; f < 9; f++) {
    const x = MARGIN + f * UNIT;
    ctx.beginPath();
    ctx.moveTo(x, MARGIN);
    ctx.lineTo(x, MARGIN + 4 * UNIT);
    ctx.moveTo(x, MARGIN + 5 * UNIT);
    ctx.lineTo(x, MARGIN + 9 * UNIT);
    ctx.stroke();
  }
  // 九宫斜线
  ctx.beginPath();
  ctx.moveTo(MARGIN + 3 * UNIT, MARGIN);
  ctx.lineTo(MARGIN + 5 * UNIT, MARGIN + 2 * UNIT);
  ctx.moveTo(MARGIN + 5 * UNIT, MARGIN);
  ctx.lineTo(MARGIN + 3 * UNIT, MARGIN + 2 * UNIT);
  ctx.moveTo(MARGIN + 3 * UNIT, MARGIN + 9 * UNIT);
  ctx.lineTo(MARGIN + 5 * UNIT, MARGIN + 7 * UNIT);
  ctx.moveTo(MARGIN + 5 * UNIT, MARGIN + 9 * UNIT);
  ctx.lineTo(MARGIN + 3 * UNIT, MARGIN + 7 * UNIT);
  ctx.stroke();
  // 河界文字
  ctx.fillStyle = "#5a3a1a";
  ctx.font = "28px 'Noto Serif SC','KaiTi','STKaiti',serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("楚 河", MARGIN + 2 * UNIT, MARGIN + 4.5 * UNIT);
  ctx.fillText("汉 界", MARGIN + 6 * UNIT, MARGIN + 4.5 * UNIT);

  // 棋子
  if (board) {
    for (let rank = 0; rank < 10; rank++) {
      for (let file = 0; file < 9; file++) {
        const cell = board[rank][file];
        if (!cell) continue;
        const { x, y } = pieceXY({ file, rank });
        const isSelected = selected && selected.file === file && selected.rank === rank;
        // 棋子底色
        ctx.beginPath();
        ctx.arc(x, y, PIECE_R, 0, Math.PI * 2);
        if (cell.color === "red") {
          ctx.fillStyle = "#f7e8c8";
          ctx.strokeStyle = "#b03a2e";
        } else {
          ctx.fillStyle = "#3a3a3a";
          ctx.strokeStyle = "#1f1f1f";
        }
        ctx.lineWidth = 2.4;
        ctx.fill();
        ctx.stroke();
        // 选中圈
        if (isSelected) {
          ctx.beginPath();
          ctx.arc(x, y, PIECE_R + 6, 0, Math.PI * 2);
          ctx.strokeStyle = "#2e7d32";
          ctx.lineWidth = 3.2;
          ctx.stroke();
        }
        // 棋子文字
        ctx.fillStyle = cell.color === "red" ? "#b03a2e" : "#f0e6d2";
        ctx.font = `bold 44px 'Noto Serif SC','KaiTi','STKaiti',serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(cell.kind, x, y + 2);
      }
    }
  }

  // 合法着目标点（targets 存 2 字符目标点）
  ctx.fillStyle = "rgba(46,125,50,0.75)";
  for (const key of targets) {
    if (!/^[a-i][0-9]$/.test(key)) continue;
    const to: Point = {
      file: key.charCodeAt(0) - "a".charCodeAt(0),
      rank: Number(key[1]),
    };
    const { x, y } = pieceXY(to);
    const targetCell = board?.[to.rank][to.file];
    if (targetCell) {
      ctx.beginPath();
      ctx.arc(x, y, PIECE_R, 0, Math.PI * 2);
      ctx.strokeStyle = "rgba(46,125,50,0.9)";
      ctx.lineWidth = 3;
      ctx.stroke();
    } else {
      ctx.beginPath();
      ctx.arc(x, y, 9, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

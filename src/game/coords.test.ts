import { describe, expect, it } from "vitest";
import { fenBoardSegment, parseFenBoard, pointToUcci, ucciToMove } from "./coords";

const START_FEN = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

describe("ucciToMove", () => {
  it("解析合法 4 字符着法", () => {
    expect(ucciToMove("h2e2")).toEqual({ from: { file: 7, rank: 2 }, to: { file: 4, rank: 2 } });
  });
  it("拒绝非法输入", () => {
    expect(ucciToMove("x2e2")).toBeNull();
    expect(ucciToMove("h2e")).toBeNull();
    expect(ucciToMove("h2e22")).toBeNull();
    expect(ucciToMove("")).toBeNull();
  });
  it("边界坐标", () => {
    expect(ucciToMove("a0i9")).toEqual({ from: { file: 0, rank: 0 }, to: { file: 8, rank: 9 } });
  });
});

describe("pointToUcci", () => {
  it("点转 2 字符", () => {
    expect(pointToUcci({ file: 0, rank: 9 })).toBe("a9");
    expect(pointToUcci({ file: 8, rank: 0 })).toBe("i0");
  });
});

describe("parseFenBoard", () => {
  it("初始局面 9×10，位置正确", () => {
    const b = parseFenBoard(fenBoardSegment(START_FEN));
    expect(b).toHaveLength(10);
    expect(b[0][0]).toEqual({ kind: "车", color: "black" }); // rank9 黑车 a
    expect(b[9][8]).toEqual({ kind: "车", color: "red" }); // rank0 红车 i
    expect(b[2][1]).toEqual({ kind: "炮", color: "black" }); // b7 黑炮
    expect(b[7][1]).toEqual({ kind: "炮", color: "red" }); // b2 红炮
    expect(b[0][4]).toEqual({ kind: "将", color: "black" });
    expect(b[9][4]).toEqual({ kind: "帅", color: "red" });
    expect(b[9][0]).toEqual({ kind: "车", color: "red" });
    // 空位
    expect(b[4][0]).toBeNull();
    expect(b[5][8]).toBeNull();
  });
  it("拒绝列数错误", () => {
    expect(() => parseFenBoard("3Rk4/3R5/6N2/9/9/4P4/9/9/9/4K4".split(" ")[0])).toThrow();
  });
  it("行数与 FEN 一致（第 1 行=黑方底线）", () => {
    // 黑将在第 1 行第 5 列（e9）
    const b = parseFenBoard(fenBoardSegment(START_FEN));
    expect(b[0][4]!.kind).toBe("将");
  });
});

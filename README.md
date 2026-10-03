# 中国象棋（Tauri）

基于 **Tauri 2** 的跨平台中国象棋应用，支持 Windows / macOS / Linux / Android。

## 功能

- **人机对战**：玩家 vs 内置引擎（难度 1–6）/ 大模型引擎 / 皮卡鱼（桌面端）
- **机器对战**：引擎 vs 引擎自动对弈，支持调速、暂停、连赛统计
- **持久化**：5 个存档槽位、自动保存与重启恢复、FEN 导入导出
- **大模型引擎**：OpenAI 兼容端点（自配 base_url / key / model），超时或出错自动降级内置引擎，永不走出非法着
- **内置引擎**：纯 Rust 实现（规则核心 + 迭代加深 α-β 搜索 + 局面评估），离线可用、Android 可用
- **皮卡鱼增强**（桌面端）：接入开源第一引擎 Pikafish（GPLv3，详见 `docs/THIRD_PARTY_LICENSES.md`）

## 快速开始

前置：Node.js ≥ 20、Rust（stable）、平台系统依赖（见对应文档）。

```bash
git clone <地址> xiangqi-tauri
cd xiangqi-tauri
npm install
npm run tauri dev        # 开发调试
cargo test --workspace   # Rust 单元/集成测试
npm run test             # 前端测试
npm run tauri build      # 打包安装包
```

## 架构

```
前端 React+TS+Vite（Canvas 棋盘） ⇄ Tauri IPC ⇄ Rust：
  xiangqi-core（规则） → xiangqi-search（搜索）
  llm-engine（大模型，超时降级） · pikafish-adapter（UCI 子进程）
  game-core（对局状态机） · game-store（持久化）
```

## 文档索引

| 文档 | 说明 |
|---|---|
| `docs/设计方案.md` | 详细设计（数据结构、协议、接口、测试矩阵） |
| `docs/开发环境部署-Windows.md` | Windows 开发/测试/打包环境 |
| `docs/开发环境部署-macOS.md` | macOS 开发/测试/打包环境（含签名说明） |
| `docs/开发环境部署-Linux.md` | Linux 开发/测试/打包环境（含发行版依赖） |
| `docs/Android-构建与测试.md` | Android SDK/NDK 配置与真机/模拟器测试 |
| `docs/测试用例清单.md` | 用户人工验收用例（P0/P1，分平台） |
| `docs/大模型配置指南.md` | 大模型接入说明（各服务商示例） |
| `docs/THIRD_PARTY_LICENSES.md` | 第三方开源声明（皮卡鱼 GPLv3） |

## 测试门禁（提交前）

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test --workspace
npm run test
```

## 开源许可

- 本项目代码：MIT OR Apache-2.0
- 皮卡鱼（Pikafish）：GPL-3.0（桌面端增强引擎，见 `docs/THIRD_PARTY_LICENSES.md`）

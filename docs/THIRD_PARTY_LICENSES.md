# 第三方开源软件声明（THIRD PARTY LICENSES）

## Pikafish（皮卡鱼）—— 中国象棋引擎

- 项目：Pikafish（源自 Stockfish 的中国象棋移植）
- 许可证：**GPL-3.0-only**
- 官网：https://www.pikafish.com
- 源码：https://github.com/official-pikafish/Pikafish
- 使用方式：本应用（桌面端 Windows/macOS/Linux）以**子进程 + UCI 协议**方式调用官方发布的引擎二进制与 NNUE 权重文件；未修改其源码；引擎资源不纳入本仓库 git 管理，由 `scripts/fetch-pikafish.sh` 或用户按 `src-tauri/resources/pikafish/README.md` 指引获取。
- 义务遵守：依据 GPLv3，随应用分发/部署时提供本声明、完整许可证文本与源码获取途径（上方链接）；应用内"关于"页亦有声明。

## GPL-3.0 许可证全文

完整文本见 https://www.gnu.org/licenses/gpl-3.0.txt ；本仓库根目录另存 `LICENSES/GPL-3.0.txt`（可由脚本获取）。

> 本应用的其余代码按仓库根目录 LICENSE（MIT OR Apache-2.0）授权。

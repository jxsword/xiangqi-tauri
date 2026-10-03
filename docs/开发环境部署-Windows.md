# 开发环境部署 —— Windows

> 适用范围：在 Windows 10/11（64 位）上开发、调试与打包本项目的桌面端（Tauri 2）
> 目标产物：`npm run tauri dev`（开发调试）/ `npm run tauri build`（打包 .msi / .exe）

## 1. 安装前置软件

### 1.1 Git
- 下载安装：https://git-scm.com/download/win（默认选项即可）
- 验证：`git --version`

### 1.2 Node.js（≥ 20）
- 下载安装：https://nodejs.org（LTS 版即可）
- 验证：`node -v`、`npm -v`

### 1.3 Rust（MSVC 工具链）
- 下载安装：https://rustup.rs（rustup-init.exe，**默认选择 MSVC ABI 选项**）
- 安装完成后重开终端，验证：`rustc -V`、`cargo -V`

### 1.4 Visual Studio Build Tools（C++ 编译必需）
- 下载：https://visualstudio.microsoft.com/downloads/ → "生成工具"（Build Tools）
- 安装工作负载：**使用 C++ 的桌面开发**（Desktop development with C++，含 MSVC 编译器、Windows SDK、CMake）
- 或在完整 Visual Studio 安装器中勾选同一工作负载
- 验证：`rustup show` 应显示 `stable-x86_64-pc-windows-msvc`

### 1.5 WebView2 运行时
- Windows 10/11 系统自带 WebView2（Edge 内核），无需额外安装
- 若目标机是 Win7/Win8 或精简系统：安装 Evergreen Bootstrapper（https://developer.microsoft.com/microsoft-edge/webview2/）

## 2. 获取与配置项目

```powershell
git clone <项目地址> xiangqi-tauri
cd xiangqi-tauri

# 安装前端依赖（同时安装 tauri CLI）
npm install

# （可选）获取皮卡鱼引擎二进制，见 scripts/fetch-pikafish.sh
```

## 3. 开发调试

```powershell
npm run tauri dev
```

- 首次运行会编译 Rust 后端（数分钟），随后打开"中国象棋"窗口
- 前端修改热更新（Vite @ 1420 端口）；Rust 修改需重新编译自动重启

## 4. 单元测试 / 门禁

```powershell
# Rust 全量测试
cargo test --workspace

# 门禁（提交前）：
cargo fmt --check
cargo clippy -- -D warnings
cargo test --workspace
npm run test
```

## 5. 打包

```powershell
npm run tauri build
```

- 产物在 `src-tauri/target/release/bundle/`：`msi/`（WiX 安装包）与 `nsis/`（安装程序 exe）
- 打包需要图标（项目已含 `src-tauri/icons/`）；若替换图标后重新生成全部尺寸：`npm run tauri icon <图标.png>`

## 6. 常见问题排查

| 现象 | 原因与解决 |
|---|---|
| `link.exe` not found / MSVC 链接错误 | 未安装 Build Tools 的 C++ 工作负载 → 按 1.4 安装并重启终端 |
| `cargo` 不是内部或外部命令 | 未加入 PATH：`%USERPROFILE%\.cargo\bin`，重开终端 |
| 提示缺少 WebView2 | 系统过旧：安装 Evergreen WebView2 运行时 |
| 构建报错 `Icon ... not found` | `src-tauri/icons/` 缺失 → 运行 `npm run tauri icon <源图>` 生成 |
| 杀毒软件拦截构建产物 | 开发阶段将 target 目录加入排除列表 |
| `npm run tauri dev` 窗口空白 | Vite 端口被占用：结束占用 1420 端口的进程后重试 |

## 7. 相关文档

- macOS：`docs/开发环境部署-macOS.md`；Linux：`docs/开发环境部署-Linux.md`
- Android：`docs/Android-构建与测试.md`
- 测试用例：`docs/测试用例清单.md`

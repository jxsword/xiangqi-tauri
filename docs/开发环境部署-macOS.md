# 开发环境部署 —— macOS

> 适用范围：在 macOS（≥ 10.15 Catalina；推荐 macOS 12+）上开发、调试与打包桌面端
> 目标产物：`npm run tauri dev` / `npm run tauri build`（.app / .dmg）
> 说明：Apple Silicon（M 系列）与 Intel 需分别出包；引擎资源目录含 `macos-arm64` 与 `macos-x64`

## 1. 安装前置软件

### 1.1 Xcode Command Line Tools
```bash
xcode-select --install
```
- 首次可能需要接受许可：`sudo xcodebuild -license accept`

### 1.2 Git / Node.js
```bash
git --version        # macOS 自带或从 https://git-scm.com 安装
node -v              # ≥ 20；从 https://nodejs.org 或 brew install node
```

### 1.3 Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc -V && cargo -V
```
- rustup 默认包含本机架构 target（`aarch64-apple-darwin` 或 `x86_64-apple-darwin`）

### 1.4 （可选）Homebrew
```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

## 2. 获取与配置项目

```bash
git clone <项目地址> xiangqi-tauri
cd xiangqi-tauri
npm install
# 可选：scripts/fetch-pikafish.sh 获取皮卡鱼二进制（选对 macos-x64 / macos-arm64）
```

## 3. 开发调试

```bash
npm run tauri dev
```

## 4. 单元测试 / 门禁

```bash
cargo test --workspace
cargo fmt --check
cargo clippy -- -D warnings
npm run test
```

## 5. 打包

```bash
npm run tauri build
```
- 产物：`src-tauri/target/release/bundle/macos/`（.app、.dmg）
- **签名与公证（阶段一可跳过）**：
  - 未签名产物在他人机器打开时被 Gatekeeper 拦截 → 右键打开 → "打开"；或 `xattr -dr com.apple.quarantine <app>`
  - 正式分发需 Apple Developer 证书：`codesign --deep --force --options runtime -s "Developer ID Application: 你的团队" <app>`，再 `xcrun notarytool submit` 公证
  - Tauri 自动签名：配置 `tauri.conf.json` → `bundle.macOS.signingIdentity`

## 6. 常见问题排查

| 现象 | 原因与解决 |
|---|---|
| 提示 `clang: error: ... SDK` | Xcode CLT 未装/未更新：`xcode-select --install` |
| 构建报 SSL 证书问题 | 更新 rustup/cargo：`rustup self update`；检查系统时间 |
| 打开 dmg 提示"已损坏" | 下载文件被 quarantine：`xattr -dr com.apple.quarantine 文件` |
| 找不到 lib（dylib） | 项目链接系统框架缺失：确认 Xcode CLT 完整 |

## 7. 相关文档

- Windows：`docs/开发环境部署-Windows.md`；Linux：`docs/开发环境部署-Linux.md`
- Android：`docs/Android-构建与测试.md`；测试用例：`docs/测试用例清单.md`

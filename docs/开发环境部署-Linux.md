# 开发环境部署 —— Linux

> 适用范围：主流 Linux 发行版（Debian/Ubuntu、Fedora、Arch、openSUSE 等）开发、调试与打包
> 目标产物：`npm run tauri dev` / `npm run tauri build`（deb / AppImage / rpm）

## 1. 系统依赖（按发行版安装）

### Debian / Ubuntu
```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libgtk-3-dev
```

### Fedora
```bash
sudo dnf install -y webkit2gtk4.1-devel gtk3-devel \
  openssl-devel glib2-devel librsvg2-devel \
  libxdo-devel libayatana-appindicator-devel \
  gcc gcc-c++ make pkg-config
```

### Arch / Manjaro
```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 pango cairo \
  libxdo librsvg libappindicator-gtk3 \
  base-devel curl wget
```

### openSUSE
```bash
sudo zypper install -y webkit2gtk3-devel gtk3-devel \
  libxdo-devel librsvg-devel libappindicator3-devel \
  gcc gcc-c++ make
```

## 2. 基础工具

```bash
# Git
sudo apt install git          # 或其他发行版对应命令

# Node.js ≥ 20（建议用 nvm 或官方二进制）
# 以官方 tarball 为例：
curl -fsSL https://nodejs.org/dist/v22.23.2/node-v22.23.2-linux-x64.tar.xz | sudo tar -xJ -C /usr/local --strip-components=1
node -v && npm -v

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc -V && cargo -V
```

## 3. 获取与配置项目

```bash
git clone <项目地址> xiangqi-tauri
cd xiangqi-tauri
npm install
# 可选：scripts/fetch-pikafish.sh 获取皮卡鱼（linux-x64）
```

## 4. 开发调试

```bash
npm run tauri dev
```

- 无桌面环境（纯服务器/CI）时可用虚拟显示：`xvfb-run -a npm run tauri dev`（构建可无显示完成，运行 UI 需桌面）

## 5. 单元测试 / 门禁

```bash
cargo test --workspace
cargo fmt --check
cargo clippy -- -D warnings
npm run test
```

## 6. 打包

```bash
npm run tauri build
```
- 产物：`src-tauri/target/release/bundle/` → `deb/`、`rpm/`（如装 rpm）、`appimage/`
- AppImage 运行需 FUSE：`sudo apt install libfuse2`（较新发行版用 libfuse2t64）

## 7. 常见问题排查

| 现象 | 原因与解决 |
|---|---|
| 找不到 `webkit2gtk-4.1` | 发行版源过旧（如 Ubuntu 20.04 需手动加源或用 webkit2gtk-4.0）→ 升级发行版或换 4.1 源 |
| 编译报 `GLib`/`GObject` 头文件缺失 | 安装 `libglib2.0-dev`、`libgdk-pixbuf-2.0-dev` |
| AppImage 无法启动 | 缺 FUSE：`sudo apt install libfuse2`；或解包运行 `./AppImage --appimage-extract-and-run` |
| 中文显示为方块 | 系统缺中文字体：`sudo apt install fonts-noto-cjk` |
| Wayland 下窗口异常 | 设 `WEBKIT_DISABLE_COMPOSITING_MODE=1` 或改用 X11/XWayland 会话 |

## 8. 相关文档

- Windows：`docs/开发环境部署-Windows.md`；macOS：`docs/开发环境部署-macOS.md`
- Android：`docs/Android-构建与测试.md`；测试用例：`docs/测试用例清单.md`

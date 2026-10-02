# Android 构建与测试

> 本项目基于 Tauri 2 官方 Android 支持。Android 端引擎可用：**内置引擎**、**大模型引擎**（LLM）；**皮卡鱼不可用**（C++ 子进程受 Android Q+ 限制，UI 自动隐藏该选项）。
> 前置条件：任一桌面平台环境已能运行 `npm run tauri dev`（见对应平台部署文档）。

## 1. 安装 Android 工具链

### 1.1 Android Studio（含 SDK）
- 下载安装：https://developer.android.com/studio
- 启动后打开 SDK Manager（Settings → Languages & Frameworks → Android SDK），安装：
  - Android SDK Platform（如 android-35）
  - Android SDK Platform-Tools
  - NDK (Side by side)（Tauri 要求 ≥ 25.x，文档示例 25.0.8775105）
  - Android SDK Build-Tools
  - Android SDK Command-line Tools

### 1.2 配置环境变量

**Windows（PowerShell）**
```powershell
[System.Environment]::SetEnvironmentVariable("JAVA_HOME", "C:\Program Files\Android\Android Studio\jbr", "User")
[System.Environment]::SetEnvironmentVariable("ANDROID_HOME", "$env:LOCALAPPDATA\Android\Sdk", "User")
[System.Environment]::SetEnvironmentVariable("NDK_HOME", "$env:ANDROID_HOME\ndk\25.0.8775105", "User")
```

**macOS / Linux**
```bash
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"   # macOS
export ANDROID_HOME="$HOME/Library/Android/sdk"                                    # macOS
export ANDROID_HOME="$HOME/Android/Sdk"                                            # Linux
export NDK_HOME="$ANDROID_HOME/ndk/25.0.8775105"
# 写入 shell 配置（~/.zshrc / ~/.bashrc）后 source
```

### 1.3 Rust Android 交叉编译目标
```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
```

## 2. 生成与构建

```bash
cd xiangqi-tauri
# 首次生成 Android 工程（会创建 src-tauri/gen/android）
npm run tauri android init

# 构建 debug APK
npm run tauri android build -- --debug

# 或直接连设备运行
npm run tauri android dev
```

- 产物：`src-tauri/gen/android/.../app/build/outputs/apk/.../app-debug.apk`

## 3. 在设备/模拟器上运行

### 模拟器
- Android Studio → Device Manager → 创建虚拟设备（Pixel 系列 + 系统镜像）
- `npm run tauri android dev` 会自动部署到已启动的模拟器

### 真机（USB 调试）
1. 手机：设置 → 关于手机 → 连点"版本号"7 次 → 开发者选项 → 开启"USB 调试"
2. 连接电脑，允许调试授权：`adb devices` 应看到设备
3. `npm run tauri android dev`

## 4. 端上测试

- Android 端执行 `docs/测试用例清单.md` 中标 **Android** 的用例（P0/P1）
- 特别关注：LLM 模式（需可联网与填写 base_url/key）、自动恢复对局、机器对战（内置 vs 内置）、深浅色背景下的棋盘可读性

## 5. 常见问题

| 现象 | 解决 |
|---|---|
| `JAVA_HOME` 无效 | 按 1.2 重设并重开终端 |
| NDK 版本报错 | 与 `tauri.conf.json`/Gradle 要求对齐，安装指定版本 NDK |
| 构建慢 | 首次构建需编译全部 Rust 依赖；后续增量 |
| 设备连不上 adb | 安装 Platform-Tools、重插 USB、检查设备授权弹窗 |
| 模拟器无网络 | AVD 配置勾选"启动时使用宿主机网络"或调整代理 |

## 6. 相关文档
- 平台部署：`docs/开发环境部署-Windows.md` / `-macOS.md` / `-Linux.md`
- 用户测试：`docs/测试用例清单.md`

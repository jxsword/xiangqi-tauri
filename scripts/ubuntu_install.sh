#!/usr/bin/env bash
# =============================================================================
# 中国象棋 (xiangqi) — Ubuntu 一键安装/升级脚本
#
# 用法:
#   ./scripts/ubuntu_install.sh                  # 构建产物存在则直接安装最新 .deb；不存在则先自动构建再安装
#   ./scripts/ubuntu_install.sh /path/xx.deb     # 指定 deb 包安装（路径不存在时报错，不自动构建）
#   ./scripts/ubuntu_install.sh --rebuild        # 强制重新构建（忽略已有 .deb）后安装；可简写 -r
#
# 安装方式优先级（按顺序尝试，前一个失败自动回退下一个）:
#   1) sudo gdebi <deb>            —— 自动解析依赖，推荐
#   2) sudo apt install ./<deb>    —— 依赖正常时等效
#
# 特性:
#   - 自动定位构建产物目录 (target/release/bundle/deb) 中的最新包
#   - 升级覆盖安装已装旧版本，无需先卸载
#   - 检测正在运行的应用并给出退出提示
# =============================================================================
set -euo pipefail

# 仓库根目录（脚本所在目录的上一级）
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
log()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
die()  { echo -e "${RED}[✗]${NC} $*" >&2; exit 1; }

# ---------- 1. 参数解析与定位 deb 包 ----------
FORCE_REBUILD=0
case "${1:-}" in
  --rebuild|-r) FORCE_REBUILD=1; DEB="" ;;
  *) DEB="${1:-}" ;;
esac

build_deb() {
  log "执行构建: npx tauri build --bundles deb（约 5-15 分钟）..."
  (cd "$ROOT" && npx tauri build --bundles deb) \
    || die "自动构建失败，请检查上方构建日志"
  local d="$(ls -t "$ROOT"/target/release/bundle/deb/*.deb 2>/dev/null | head -1)"
  [[ -n "$d" && -f "$d" ]] || \
    die "构建未生成 .deb 包。请手动构建后重试: cd $ROOT && npx tauri build --bundles deb"
  DEB="$d"
}

if [[ "$FORCE_REBUILD" -eq 1 ]]; then
  log "强制重建模式：忽略已有 .deb，重新构建..."
  build_deb
elif [[ -z "$DEB" ]]; then
  # 常见查找目录：脚本上级的 target 产物 + 当前目录
  CANDIDATES=()
  for dir in "$ROOT/target/release/bundle/deb" "$PWD" "$ROOT"; do
    [[ -d "$dir" ]] || continue
    while IFS= read -r -d '' f; do CANDIDATES+=("$f"); done \
      < <(find "$dir" -maxdepth 1 -name '*.deb' -type f -print0)
  done
  if [[ ${#CANDIDATES[@]} -gt 0 ]]; then
    # 按修改时间排序取最新
    DEB="$(printf '%s\n' "${CANDIDATES[@]}" | xargs -r ls -t 2>/dev/null | head -1)"
  fi
  if [[ -n "$DEB" && -f "$DEB" ]]; then
    log "找到构建产物: $DEB"
    # 陈旧检测：deb 修改时间早于 git 最新提交时间 => 源码已更新未打包，自动重建
    HEAD_TIME="$(git -C "$ROOT" log -1 --format='%ct' 2>/dev/null || echo 0)"
    DEB_TIME="$(stat -c '%Y' "$DEB" 2>/dev/null || echo 0)"
    if [[ "$HEAD_TIME" -gt 0 && "$DEB_TIME" -gt 0 && "$DEB_TIME" -lt "$HEAD_TIME" ]]; then
      warn "检测到 $DEB 早于源码最新提交（源码有更新但未重新打包），自动重建（约 5-15 分钟）..."
      build_deb
      log "自动重建完成: $DEB"
    fi
  else
    warn "构建目录中未找到 .deb 安装包，将自动执行构建（首次构建较慢，约 5-15 分钟）..."
    build_deb
    log "自动构建完成: $DEB"
  fi
fi
[[ -f "$DEB" ]] || die "deb 包不存在: $DEB"

log "使用安装包: $DEB"
echo "   包信息:"
dpkg-deb --info "$DEB" | grep -E '^\s*(Package|Version|Architecture|Description):' | sed 's/^/     /' || true

# ---------- 2. 升级前检查运行实例 ----------
if pgrep -x xiangqi >/dev/null 2>&1; then
  warn "检测到中国象棋 (xiangqi) 正在运行，升级前建议先退出应用。"
  warn "5 秒后继续安装（Ctrl+C 可取消）..."
  sleep 5
fi

# ---------- 3. 安装方式 1: gdebi ----------
install_gdebi() {
  if command -v gdebi >/dev/null 2>&1; then
    log "使用 sudo gdebi 安装（自动处理依赖）..."
    sudo gdebi --non-interactive "$DEB"
  else
    warn "未安装 gdebi，先安装 gdebi-core ..."
    if sudo apt-get install -y gdebi-core >/dev/null 2>&1; then
      log "使用 sudo gdebi 安装 ..."
      sudo gdebi --non-interactive "$DEB"
    else
      return 1
    fi
  fi
}

# ---------- 4. 安装方式 2: apt install ./xxx.deb ----------
install_apt() {
  log "回退使用 sudo apt install ./$(basename "$DEB") 安装 ..."
  sudo apt-get install -y "$DEB"
}

if ! install_gdebi; then
  warn "gdebi 方式失败，回退 apt 方式 ..."
  install_apt
fi

log "安装/升级完成！"
if command -v xiangqi >/dev/null 2>&1; then
  log "运行应用: xiangqi"
  log "（WSL 下白屏/无窗口时，请参考 docs/ 中的启动白屏解决方案）"
else
  warn "未找到 xiangqi 命令；请确认安装输出无错误，或重新运行本脚本。"
fi

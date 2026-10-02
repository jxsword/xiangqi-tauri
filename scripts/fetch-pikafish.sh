#!/usr/bin/env bash
# 获取皮卡鱼引擎资源（桌面端）
# 用法：
#   scripts/fetch-pikafish.sh                  # 检测本机平台并给出下载指引/尝试自动下载
#   scripts/fetch-pikafish.sh --url <tar-url>  # 从指定官方发布 tar 包安装
#
# 说明：皮卡鱼官方发布页 https://github.com/official-pikafish/Pikafish/releases
# 资产命名随版本变化，脚本优先使用用户提供的 URL；无 URL 时打印指引。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/src-tauri/resources/pikafish"

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os" in
    linux*) os=linux ;;
    darwin*) os=macos ;;
    mingw*|msys*|cygwin*) os=windows ;;
    *) echo "unsupported os: $os" >&2; exit 1 ;;
  esac
  case "$arch" in
    x86_64|amd64) arch=x64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo "unsupported arch: $arch" >&2; exit 1 ;;
  esac
  echo "${os}-${arch}"
}

main() {
  mkdir -p "$DEST"
  local url="${1:-}"

  if [[ -n "$url" ]]; then
    local plat="$2"   # 期望目标目录名，如 linux-x64
    local dir="$DEST/$plat"
    mkdir -p "$dir"
    echo ">> 下载 $url -> $dir"
    curl -L --proto '=https' --tlsv1.2 -sSf "$url" -o /tmp/pikafish-dl.tar.xz
    tar -xJf /tmp/pikafish-dl.tar.xz -C "$dir"
    echo ">> 完成，请确认 $dir 下包含引擎可执行文件与 .nnue 权重"
    return 0
  fi

  local plat
  plat="$(detect_platform)"
  echo ">> 未提供 URL。请在官方发布页下载 ${plat} 版本："
  echo "   https://github.com/official-pikafish/Pikafish/releases"
  echo "   下载后解压到： $DEST/${plat}/"
  echo "   需包含：可执行文件（pikafish / pikafish.exe）与 pikafish.nnue"
  echo "   然后可用：scripts/fetch-pikafish.sh --url <tar包URL> ${plat}"
}

main "$@"

#!/usr/bin/env bash
# 在 macOS 上一键构建 全球网测 桌面版 .dmg
#
#   ./build-macos.sh            # 只构建当前架构（Apple Silicon 或 Intel）
#   ./build-macos.sh universal  # 构建 arm64 + x86_64 通用包
#
# 前置：Xcode Command Line Tools（xcode-select --install）、Rust（rustup）
set -euo pipefail
cd "$(dirname "$0")"

echo "==> 检查工具链"
if ! command -v cargo >/dev/null 2>&1; then
  echo "未检测到 Rust。先安装："
  echo '  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh'
  exit 1
fi
if ! xcode-select -p >/dev/null 2>&1; then
  echo "未检测到 Xcode Command Line Tools：xcode-select --install"
  exit 1
fi

echo "==> 安装前端打包 CLI（首次需要，约 10MB）"
if [ ! -d node_modules ]; then
  if command -v pnpm >/dev/null 2>&1; then
    pnpm install
  else
    npm install
  fi
fi

MODE="${1:-native}"
if [ "$MODE" = "universal" ]; then
  echo "==> 通用包构建（aarch64 + x86_64）"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  npx tauri build --target universal-apple-darwin --bundles dmg
  OUT="src-tauri/target/universal-apple-darwin/release/bundle/dmg"
else
  case "$(uname -m)" in
    arm64)  TRIPLE=aarch64-apple-darwin ;;
    x86_64) TRIPLE=x86_64-apple-darwin ;;
    *) echo "未知架构：$(uname -m)"; exit 1 ;;
  esac
  echo "==> 构建 $TRIPLE"
  npx tauri build --target "$TRIPLE" --bundles dmg
  OUT="src-tauri/target/$TRIPLE/release/bundle/dmg"
fi

echo
echo "==> 完成，产物："
ls -lh "$OUT"
echo
echo "首次打开（未签名，Gatekeeper 会拦）：右键点 App → 打开；或执行"
echo "  xattr -dr com.apple.quarantine \"/Applications/GlobalSpeedTest.app\""

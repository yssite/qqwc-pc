#!/bin/bash
# 在 macOS 上准备构建环境：Rust(minimal) + Node.js + tauri CLI
set -x
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
export PATH="$HOME/.cargo/bin:$HOME/.local/node/bin:$PATH"

mkdir -p "$HOME/.local/bin" "$HOME/.local/node" "$HOME/gst"

if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  echo "=== rustup-init ==="
  curl -fsSL -o "$HOME/.local/bin/rustup-init" \
    https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup/dist/aarch64-apple-darwin/rustup-init || exit 11
  chmod +x "$HOME/.local/bin/rustup-init"
  "$HOME/.local/bin/rustup-init" -y --profile minimal --no-modify-path --default-toolchain stable || exit 12
fi

if [ ! -x "$HOME/.local/node/bin/node" ]; then
  echo "=== node ==="
  NODE_VER=v22.20.0
  curl -fsSL -o "$HOME/.local/node.tar.xz" \
    "https://npmmirror.com/mirrors/node/${NODE_VER}/node-${NODE_VER}-darwin-arm64.tar.xz" || exit 21
  tar -xJf "$HOME/.local/node.tar.xz" -C "$HOME/.local/node" --strip-components=1 || exit 22
  rm -f "$HOME/.local/node.tar.xz"
fi

"$HOME/.cargo/bin/cargo" --version
"$HOME/.local/node/bin/node" --version
"$HOME/.local/node/bin/npm" --version
echo "BOOTSTRAP_OK"

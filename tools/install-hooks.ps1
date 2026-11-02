# 安装 git 钩子：提交前自动跑隐私扫描
#
#   pwsh tools/install-hooks.ps1
#
# 会写入 .git/hooks/pre-commit（已存在则备份为 pre-commit.bak）
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$hooks = Join-Path $root ".git/hooks"
if (-not (Test-Path $hooks)) { throw "找不到 .git/hooks，请在仓库根目录执行" }

$target = Join-Path $hooks "pre-commit"
if (Test-Path $target) { Copy-Item $target "$target.bak" -Force }

@'
#!/bin/sh
# 提交前隐私扫描（由 tools/install-hooks.ps1 安装）
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -ExecutionPolicy Bypass -File "$(git rev-parse --show-toplevel)/tools/privacy_scan.ps1"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -ExecutionPolicy Bypass -File "$(git rev-parse --show-toplevel)/tools/privacy_scan.ps1"
else
  echo "[privacy] 未找到 pwsh/powershell，跳过扫描"
fi
'@ | Set-Content -LiteralPath $target -Encoding utf8 -NoNewline
Write-Host "[privacy] 已安装 pre-commit 钩子：$target" -ForegroundColor Green

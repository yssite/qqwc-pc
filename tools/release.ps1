# 一键打包发布：Windows 安装包 + 便携版（命名带版本号），可选创建 GitHub Release
#
#   pwsh tools/release.ps1              # 只打包
#   pwsh tools/release.ps1 -Publish     # 打包并创建/更新 GitHub Release
#
# 约定：
#   GlobalSpeedTest_<ver>_x64-setup.exe   安装包（NSIS）
#   GlobalSpeedTest_<ver>-portable.exe    便携版
param(
  [switch]$Publish,
  [string]$Notes = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# 版本号以 tauri.conf.json 为准
$conf = Get-Content "app/src-tauri/tauri.conf.json" -Raw | ConvertFrom-Json
$ver = $conf.version
Write-Host "==> 版本 $ver"

$env:PATH = "C:\rust\bin;$env:PATH"
Push-Location app
try {
  pnpm tauri build --bundles nsis
} finally {
  Pop-Location
}

New-Item -ItemType Directory -Force -Path dist | Out-Null
$setup = Get-ChildItem "app/src-tauri/target/release/bundle/nsis/*.exe" |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
Copy-Item $setup.FullName "dist/GlobalSpeedTest_${ver}_x64-setup.exe" -Force
Copy-Item "app/src-tauri/target/release/globalspeed.exe" "dist/GlobalSpeedTest_${ver}-portable.exe" -Force

# 清掉旧版本的产物，避免混在一起
Get-ChildItem dist -Filter "GlobalSpeedTest_*" | Where-Object { $_.Name -notlike "*$ver*" } | Remove-Item -Force

Write-Host "==> 产物："
Get-ChildItem dist | Select-Object Name, @{n = "MB"; e = { [math]::Round($_.Length / 1MB, 2) } } | Format-Table

if ($Publish) {
  $args = @("release", "create", "v$ver",
    "dist/GlobalSpeedTest_${ver}_x64-setup.exe",
    "dist/GlobalSpeedTest_${ver}-portable.exe")
  if (Test-Path "dist/GlobalSpeedTest_${ver}_aarch64.dmg") {
    $args += "dist/GlobalSpeedTest_${ver}_aarch64.dmg"
  }
  $args += @("--title", "v$ver", "--notes", $(if ($Notes) { $Notes } else { "见 README 变更记录" }))
  gh @args
}

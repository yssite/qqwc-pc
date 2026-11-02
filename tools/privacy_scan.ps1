# 提交前隐私扫描：拦截个人信息（位置/运营商/公网 IP/本机路径/主机名/令牌）
#
#   pwsh tools/privacy_scan.ps1            # 扫描已暂存文件（pre-commit 用）
#   pwsh tools/privacy_scan.ps1 -All       # 扫描全部已跟踪文件
#   pwsh tools/privacy_scan.ps1 -Fix       # 只报告，不改文件
#
# 退出码 0 = 通过，1 = 发现疑似隐私信息（会阻止提交）
param([switch]$All)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# 规则：正则 -> 说明
$rules = @(
  @{ p = 'C:\\Users\\[A-Za-z0-9._-]+';        m = '本机 Windows 用户目录' },
  @{ p = '/Users/[A-Za-z0-9._-]+/';           m = '本机 macOS 用户目录' },
  @{ p = '192\.168\.\d+\.\d+';                m = '内网 IP' },
  @{ p = '\b10\.\d+\.\d+\.\d+';               m = '内网 IP' },
  @{ p = 'macmini[_-]?\w*';                   m = '内网主机名' },
  @{ p = '\b(成都|四川)\b';                    m = '具体地理位置（个人）' },
  @{ p = '110\.184\.179\.\d+';                m = '个人公网 IP' },
  @{ p = 'gh[pous]_[A-Za-z0-9]{20,}';         m = 'GitHub 令牌' },
  @{ p = 'sk-[A-Za-z0-9]{20,}';               m = 'API Key' },
  @{ p = 'SESSDATA=[^;\s]{10,}';              m = 'B 站 Cookie' }
)

# 白名单：这些文件内容天然包含大量公开 IP / 省份名
$allowFiles = @(
  'app/src-tauri/resources/nodes.json',
  'tools/privacy_scan.ps1'
)

$raw = if ($All) {
  git ls-files
} else {
  git diff --cached --name-only --diff-filter=ACM
}

$files = @(
  $raw |
    ForEach-Object { ("$_" -replace "`r", "").Trim() } |
    Where-Object { $_ -and ($allowFiles -notcontains $_) -and (Test-Path -LiteralPath $_ -PathType Leaf) } |
    Select-Object -Unique
)
$hits = @()

foreach ($f in $files) {
  $item = Get-Item -LiteralPath $f -ErrorAction SilentlyContinue
  if (-not $item) { continue }
  # 二进制/大文件跳过
  if ($item.Length -gt 2MB) { continue }
  if ($f -match '\.(png|ico|icns|jpg|dmg|exe|zip|apk|woff2?)$') { continue }

  $lines = Get-Content -LiteralPath $f -Encoding utf8 -ErrorAction SilentlyContinue
  if (-not $lines) { continue }

  for ($i = 0; $i -lt $lines.Count; $i++) {
    foreach ($r in $rules) {
      if ($lines[$i] -match $r.p) {
        $hits += [PSCustomObject]@{
          File = $f
          Line = $i + 1
          Rule = $r.m
          Text = $lines[$i].Trim().Substring(0, [Math]::Min(100, $lines[$i].Trim().Length))
        }
      }
    }
  }
}

if ($hits.Count -eq 0) {
  Write-Host "[privacy] 通过：未发现疑似个人信息" -ForegroundColor Green
  exit 0
}

Write-Host "[privacy] 发现 $($hits.Count) 处疑似个人信息：" -ForegroundColor Yellow
$hits | Format-Table File, Line, Rule, Text -AutoSize | Out-String -Width 200 | Write-Host
Write-Host "如为误报，请把文件加入 tools/privacy_scan.ps1 的 `$allowFiles，或改写文案后重试。" -ForegroundColor Yellow
exit 1

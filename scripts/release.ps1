# ExcelMiner 发布构建脚本
# 构建便携版 + 生成 SHA256 校验文件

param(
    [string]$Version = "",
    [switch]$SkipChecksum
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  ExcelMiner 发布构建" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ── 版本号 ──
if ($Version) {
    Write-Host "更新版本号: $Version" -ForegroundColor Yellow
    $packageJson = Get-Content "$root\package.json" -Raw | ConvertFrom-Json
    $packageJson.version = $Version
    $packageJson | ConvertTo-Json -Depth 10 | Set-Content "$root\package.json"
    
    $tauriConf = Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json
    $tauriConf.version = $Version
    $tauriConf | ConvertTo-Json -Depth 10 | Set-Content "$root\src-tauri\tauri.conf.json"
    
    $cargoToml = Get-Content "$root\src-tauri\Cargo.toml" -Raw
    $cargoToml = $cargoToml -replace 'version = "[\d.]+"', "version = `"$Version`""
    Set-Content "$root\src-tauri\Cargo.toml" $cargoToml
    Write-Host "  版本号已更新到 $Version" -ForegroundColor Green
}

# ── 运行测试 ──
Write-Host "[1/3] 运行测试..." -ForegroundColor Yellow
& "$PSScriptRoot\run-all-tests.ps1"
if ($LASTEXITCODE -ne 0) {
    Write-Host "  ✗ 测试失败，中止构建" -ForegroundColor Red
    exit 1
}

# ── Tauri 构建 ──
Write-Host "[2/3] Tauri 构建..." -ForegroundColor Yellow
Push-Location $root
try {
    npm run tauri build
    Write-Host "  ✓ 构建完成" -ForegroundColor Green
} finally {
    Pop-Location
}

# ── 生成校验文件 ──
if (-not $SkipChecksum) {
    Write-Host "[3/3] 生成 SHA256..." -ForegroundColor Yellow
    $bundleDir = "$root\src-tauri\target\release\bundle"
    if (Test-Path $bundleDir) {
        $checksumFile = "$bundleDir\SHA256SUMS.txt"
        Get-ChildItem -Recurse -File $bundleDir -Exclude "SHA256SUMS.txt" | ForEach-Object {
            $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
            "$hash  $($_.Name)" | Out-File -Append $checksumFile -Encoding UTF8
        }
        Write-Host "  ✓ SHA256 校验文件已生成" -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  发布构建完成" -ForegroundColor Cyan
Write-Host "  产物目录: src-tauri\target\release\bundle" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# ExcelMiner 一键测试脚本
# 运行所有后端 (Rust) 和前端 (TypeScript) 测试

param(
    [switch]$SkipFrontend,
    [switch]$SkipBackend,
    [switch]$Coverage
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  ExcelMiner 全量测试" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ── 后端测试 ──
if (-not $SkipBackend) {
    Write-Host "[1/2] Rust 后端测试..." -ForegroundColor Yellow
    Push-Location "$root\src-tauri"
    try {
        if ($Coverage) {
            cargo tarpaulin --out Html --output-dir coverage
            Write-Host "  覆盖率报告: src-tauri/coverage/tarpaulin-report.html" -ForegroundColor Green
        } else {
            cargo test --all
        }
        Write-Host "  ✓ 后端测试通过" -ForegroundColor Green
    } finally {
        Pop-Location
    }
    Write-Host ""
}

# ── 前端测试 ──
if (-not $SkipFrontend) {
    Write-Host "[2/2] 前端测试..." -ForegroundColor Yellow
    Push-Location $root
    try {
        npx tsc --noEmit
        Write-Host "  ✓ TypeScript 类型检查通过" -ForegroundColor Green

        if ($Coverage) {
            npx vitest run --coverage
            Write-Host "  覆盖率报告: coverage/index.html" -ForegroundColor Green
        } else {
            npx vitest run
        }
        Write-Host "  ✓ 前端测试通过" -ForegroundColor Green
    } finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  全部测试完成" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

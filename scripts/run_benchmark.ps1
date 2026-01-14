# Hermes Benchmark Script for Windows
# ====================================
# Automated benchmark runner for Hermes message broker
#
# Usage:
#   .\run_benchmark.ps1                    # Run default benchmark
#   .\run_benchmark.ps1 -Tokens 5000       # Custom token count
#   .\run_benchmark.ps1 -Rate 500          # Custom rate
#   .\run_benchmark.ps1 -Duration 60       # Custom duration

param(
    [int]$Tokens = 1000,
    [int]$Rate = 200,
    [int]$Duration = 30,
    [switch]$SkipBuild,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  Hermes Benchmark Runner (Windows)" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Configuration
$HermesRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not (Test-Path "$HermesRoot\Cargo.toml")) {
    $HermesRoot = Get-Location
}

$ServerExe = "$HermesRoot\target\release\hermes_server.exe"
$SubscriberExe = "$HermesRoot\target\release\hermes_subscriber.exe"
$InjectorExe = "$HermesRoot\target\release\examples\battle_test.exe"

# Build if needed
if (-not $SkipBuild) {
    Write-Host "[1/5] Building Hermes (release mode)..." -ForegroundColor Yellow
    Push-Location $HermesRoot
    cargo build --release --bin hermes_server --bin hermes_subscriber --example battle_test 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build failed!" -ForegroundColor Red
        exit 1
    }
    Pop-Location
    Write-Host "      Build complete" -ForegroundColor Green
} else {
    Write-Host "[1/5] Skipping build (using existing binaries)" -ForegroundColor Yellow
}

# Check binaries exist
if (-not (Test-Path $ServerExe)) {
    Write-Host "Error: Server binary not found at $ServerExe" -ForegroundColor Red
    exit 1
}

# Kill any existing processes
Write-Host "[2/5] Cleaning up existing processes..." -ForegroundColor Yellow
Get-Process -Name "hermes_server" -ErrorAction SilentlyContinue | Stop-Process -Force
Get-Process -Name "hermes_subscriber" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
Write-Host "      Cleanup complete" -ForegroundColor Green

# Start server
Write-Host "[3/5] Starting Hermes Server..." -ForegroundColor Yellow
$serverProcess = Start-Process -FilePath $ServerExe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 2

if ($serverProcess.HasExited) {
    Write-Host "Error: Server failed to start" -ForegroundColor Red
    exit 1
}
Write-Host "      Server running (PID: $($serverProcess.Id))" -ForegroundColor Green

# Start subscriber
Write-Host "[4/5] Starting Rust Subscriber (duration: ${Duration}s)..." -ForegroundColor Yellow
$subscriberArgs = "--duration", $Duration
if ($Verbose) { $subscriberArgs += "--verbose" }

$subscriberProcess = Start-Process -FilePath $SubscriberExe -ArgumentList $subscriberArgs -PassThru -NoNewWindow -RedirectStandardOutput "$HermesRoot\subscriber_output.txt"
Start-Sleep -Seconds 3

if ($subscriberProcess.HasExited) {
    Write-Host "Error: Subscriber failed to start" -ForegroundColor Red
    Stop-Process -Id $serverProcess.Id -Force
    exit 1
}
Write-Host "      Subscriber running (PID: $($subscriberProcess.Id))" -ForegroundColor Green

# Run injector
Write-Host "[5/5] Running Battle Test Injector..." -ForegroundColor Yellow
Write-Host "      Tokens: $Tokens, Rate: $Rate msg/sec" -ForegroundColor Gray
Write-Host ""

$injectorArgs = "--tokens", $Tokens, "--rate", $Rate
& $InjectorExe @injectorArgs

Write-Host ""
Write-Host "Waiting for subscriber to complete..." -ForegroundColor Yellow

# Wait for subscriber with timeout
$timeout = $Duration + 30
$waited = 0
while (-not $subscriberProcess.HasExited -and $waited -lt $timeout) {
    Start-Sleep -Seconds 1
    $waited++
    if ($waited % 10 -eq 0) {
        Write-Host "  Still waiting... ($waited/$timeout sec)" -ForegroundColor Gray
    }
}

# Cleanup
Write-Host ""
Write-Host "Cleaning up..." -ForegroundColor Yellow
Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
if (-not $subscriberProcess.HasExited) {
    Stop-Process -Id $subscriberProcess.Id -Force -ErrorAction SilentlyContinue
}

# Display subscriber results
Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  SUBSCRIBER RESULTS" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

if (Test-Path "$HermesRoot\subscriber_output.txt") {
    Get-Content "$HermesRoot\subscriber_output.txt" | Select-Object -Last 40
    Remove-Item "$HermesRoot\subscriber_output.txt" -Force
} else {
    Write-Host "No subscriber output captured" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Benchmark complete!" -ForegroundColor Green

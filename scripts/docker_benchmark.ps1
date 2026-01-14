# Docker Benchmark Script for Hermes (PowerShell)

param(
    [int]$Tokens = 1000,
    [int]$Rate = 200,
    [int]$Duration = 60
)

Write-Host "🐳 Hermes Docker Benchmark" -ForegroundColor Cyan
Write-Host "==========================" -ForegroundColor Cyan
Write-Host "Tokens: $Tokens"
Write-Host "Rate: $Rate msg/sec"
Write-Host "Duration: $Duration sec"
Write-Host ""

# Build images
Write-Host "📦 Building Docker images..." -ForegroundColor Yellow
docker-compose build
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Build failed!" -ForegroundColor Red
    exit 1
}

# Start server
Write-Host "🚀 Starting Hermes server..." -ForegroundColor Green
docker-compose up -d hermes-server

# Wait for server to be ready
Write-Host "⏳ Waiting for server to be ready..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

# Start subscriber
Write-Host "📡 Starting subscriber..." -ForegroundColor Green
docker-compose run -d --name hermes-subscriber-test hermes-subscriber sh -c "sleep 3 && /app/hermes_subscriber --host hermes-server:9999 --duration $Duration"

# Wait for subscriber to connect
Start-Sleep -Seconds 5

# Run injector
Write-Host "💉 Starting message injector..." -ForegroundColor Green
docker-compose run --rm hermes-injector sh -c "sleep 2 && /app/battle_test --host hermes-server:9999 --tokens $Tokens --rate $Rate"

# Wait for completion
Write-Host "⏳ Waiting for test completion..." -ForegroundColor Yellow
Start-Sleep -Seconds 10

# Show logs
Write-Host ""
Write-Host "📊 Server Logs:" -ForegroundColor Cyan
Write-Host "===============" -ForegroundColor Cyan
docker-compose logs hermes-server | Select-Object -Last 20

Write-Host ""
Write-Host "📊 Subscriber Logs:" -ForegroundColor Cyan
Write-Host "===================" -ForegroundColor Cyan
docker logs hermes-subscriber-test | Select-Object -Last 30

# Cleanup
Write-Host ""
Write-Host "🧹 Cleaning up..." -ForegroundColor Yellow
docker-compose down
docker rm -f hermes-subscriber-test 2>$null

Write-Host ""
Write-Host "✅ Benchmark complete!" -ForegroundColor Green

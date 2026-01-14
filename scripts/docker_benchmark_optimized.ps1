# Optimized Docker Benchmark Script for Hermes (PowerShell)
# Uses optimized Dockerfile with kernel tuning and CPU pinning

param(
    [int]$Tokens = 1000,
    [int]$Rate = 200,
    [int]$Duration = 60
)

Write-Host "🚀 Hermes OPTIMIZED Docker Benchmark" -ForegroundColor Cyan
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "Tokens: $Tokens"
Write-Host "Rate: $Rate msg/sec"
Write-Host "Duration: $Duration sec"
Write-Host ""

Write-Host "⚡ Optimizations:" -ForegroundColor Yellow
Write-Host "  - CPU Pinning (cores 0-3)"
Write-Host "  - Kernel tuning (privileged mode)"
Write-Host "  - Native CPU optimizations"
Write-Host "  - Jumbo frames (MTU 9000)"
Write-Host ""

# Build optimized images
Write-Host "📦 Building optimized Docker images..." -ForegroundColor Yellow
docker-compose -f docker-compose.optimized.yml build
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Build failed!" -ForegroundColor Red
    exit 1
}

# Start server
Write-Host "🚀 Starting optimized Hermes server..." -ForegroundColor Green
docker-compose -f docker-compose.optimized.yml up -d hermes-server

# Wait for server to be ready
Write-Host "⏳ Waiting for server to be ready..." -ForegroundColor Yellow
Start-Sleep -Seconds 5

# Start subscriber
Write-Host "📡 Starting optimized subscriber..." -ForegroundColor Green
docker-compose -f docker-compose.optimized.yml run -d --name hermes-subscriber-opt-test hermes-subscriber sh -c "sleep 3 && /app/hermes_subscriber --host hermes-server:9999 --duration $Duration"

# Wait for subscriber to connect
Start-Sleep -Seconds 5

# Run injector
Write-Host "💉 Starting message injector..." -ForegroundColor Green
docker-compose -f docker-compose.optimized.yml run --rm hermes-injector sh -c "sleep 2 && /app/battle_test --host hermes-server:9999 --tokens $Tokens --rate $Rate"

# Wait for completion
Write-Host "⏳ Waiting for test completion..." -ForegroundColor Yellow
Start-Sleep -Seconds 10

# Show logs
Write-Host ""
Write-Host "📊 Server Logs:" -ForegroundColor Cyan
Write-Host "===============" -ForegroundColor Cyan
docker-compose -f docker-compose.optimized.yml logs hermes-server | Select-Object -Last 20

Write-Host ""
Write-Host "📊 Subscriber Logs:" -ForegroundColor Cyan
Write-Host "===================" -ForegroundColor Cyan
docker logs hermes-subscriber-opt-test | Select-Object -Last 30

# Cleanup
Write-Host ""
Write-Host "🧹 Cleaning up..." -ForegroundColor Yellow
docker-compose -f docker-compose.optimized.yml down
docker rm -f hermes-subscriber-opt-test 2>$null

Write-Host ""
Write-Host "✅ Optimized benchmark complete!" -ForegroundColor Green
Write-Host ""
Write-Host "💡 Tips for even better performance:" -ForegroundColor Yellow
Write-Host "  - Ensure Docker Desktop has 4+ CPU cores allocated"
Write-Host "  - Close other applications to reduce CPU contention"
Write-Host "  - Run multiple times and take the best result"
Write-Host "  - For production: Deploy to native Linux with full tuning"

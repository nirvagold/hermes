# Test CI Locally - Simulate GitHub Actions CI
# Run this script to verify all CI checks pass before pushing

Write-Host "🧪 Testing CI Checks Locally" -ForegroundColor Cyan
Write-Host "============================`n" -ForegroundColor Cyan

$failed = $false

# 1. Format Check
Write-Host "📝 Running Format Check..." -ForegroundColor Yellow
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Format check FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Format check PASSED`n" -ForegroundColor Green
}

# 2. Clippy Check
Write-Host "🔍 Running Clippy Check..." -ForegroundColor Yellow
cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Clippy check FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Clippy check PASSED`n" -ForegroundColor Green
}

# 3. Check with strict warnings
Write-Host "⚠️  Running Check with -Dwarnings..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-Dwarnings"
cargo check --all-targets
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Check with warnings FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Check with warnings PASSED`n" -ForegroundColor Green
}
$env:RUSTFLAGS = ""

# 4. Test Check
Write-Host "🧪 Running Tests..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-Dwarnings"
cargo test --all-targets
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Tests FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Tests PASSED`n" -ForegroundColor Green
}
$env:RUSTFLAGS = ""

# 5. Build Check
Write-Host "🔨 Building Release Binaries..." -ForegroundColor Yellow
cargo build --release --bins
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Binary build FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Binary build PASSED" -ForegroundColor Green
}

cargo build --release --examples
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Example build FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Example build PASSED`n" -ForegroundColor Green
}

# 6. Documentation Check
Write-Host "📚 Building Documentation..." -ForegroundColor Yellow
$env:RUSTDOCFLAGS = "-Dwarnings"
cargo doc --no-deps
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Documentation build FAILED" -ForegroundColor Red
    $failed = $true
} else {
    Write-Host "✅ Documentation build PASSED`n" -ForegroundColor Green
}
$env:RUSTDOCFLAGS = ""

# Summary
Write-Host "`n================================" -ForegroundColor Cyan
if ($failed) {
    Write-Host "❌ CI CHECKS FAILED" -ForegroundColor Red
    Write-Host "Fix the errors above before pushing" -ForegroundColor Red
    exit 1
} else {
    Write-Host "✅ ALL CI CHECKS PASSED!" -ForegroundColor Green
    Write-Host "Safe to push to GitHub" -ForegroundColor Green
    exit 0
}

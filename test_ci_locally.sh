#!/bin/bash
# Test CI Locally - Simulate GitHub Actions CI
# Run this script to verify all CI checks pass before pushing

set -e

echo "🧪 Testing CI Checks Locally"
echo "============================"
echo ""

failed=0

# 1. Format Check
echo "📝 Running Format Check..."
if cargo fmt --all -- --check; then
    echo "✅ Format check PASSED"
    echo ""
else
    echo "❌ Format check FAILED"
    failed=1
fi

# 2. Clippy Check
echo "🔍 Running Clippy Check..."
if cargo clippy --all-targets -- -D warnings; then
    echo "✅ Clippy check PASSED"
    echo ""
else
    echo "❌ Clippy check FAILED"
    failed=1
fi

# 3. Check with strict warnings
echo "⚠️  Running Check with -Dwarnings..."
if RUSTFLAGS="-Dwarnings" cargo check --all-targets; then
    echo "✅ Check with warnings PASSED"
    echo ""
else
    echo "❌ Check with warnings FAILED"
    failed=1
fi

# 4. Test Check
echo "🧪 Running Tests..."
if RUSTFLAGS="-Dwarnings" cargo test --all-targets; then
    echo "✅ Tests PASSED"
    echo ""
else
    echo "❌ Tests FAILED"
    failed=1
fi

# 5. Build Check
echo "🔨 Building Release Binaries..."
if cargo build --release --bins; then
    echo "✅ Binary build PASSED"
else
    echo "❌ Binary build FAILED"
    failed=1
fi

if cargo build --release --examples; then
    echo "✅ Example build PASSED"
    echo ""
else
    echo "❌ Example build FAILED"
    failed=1
fi

# 6. Documentation Check
echo "📚 Building Documentation..."
if RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps; then
    echo "✅ Documentation build PASSED"
    echo ""
else
    echo "❌ Documentation build FAILED"
    failed=1
fi

# Summary
echo ""
echo "================================"
if [ $failed -eq 1 ]; then
    echo "❌ CI CHECKS FAILED"
    echo "Fix the errors above before pushing"
    exit 1
else
    echo "✅ ALL CI CHECKS PASSED!"
    echo "Safe to push to GitHub"
    exit 0
fi

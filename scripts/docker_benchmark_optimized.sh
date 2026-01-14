#!/bin/bash
# Optimized Docker Benchmark Script for Hermes
# Uses optimized Dockerfile with kernel tuning and CPU pinning

set -e

TOKENS=${1:-1000}
RATE=${2:-200}
DURATION=${3:-60}

echo "🚀 Hermes OPTIMIZED Docker Benchmark"
echo "====================================="
echo "Tokens: $TOKENS"
echo "Rate: $RATE msg/sec"
echo "Duration: $DURATION sec"
echo ""

echo "⚡ Optimizations:"
echo "  - CPU Pinning (cores 0-3)"
echo "  - Kernel tuning (privileged mode)"
echo "  - Native CPU optimizations"
echo "  - Jumbo frames (MTU 9000)"
echo ""

# Build optimized images
echo "📦 Building optimized Docker images..."
docker-compose -f docker-compose.optimized.yml build

# Start server
echo "🚀 Starting optimized Hermes server..."
docker-compose -f docker-compose.optimized.yml up -d hermes-server

# Wait for server to be ready
echo "⏳ Waiting for server to be ready..."
sleep 5

# Start subscriber
echo "📡 Starting optimized subscriber..."
docker-compose -f docker-compose.optimized.yml run -d --name hermes-subscriber-opt-test \
  hermes-subscriber \
  sh -c "sleep 3 && /app/hermes_subscriber --host hermes-server:9999 --duration $DURATION"

# Wait for subscriber to connect
sleep 5

# Run injector
echo "💉 Starting message injector..."
docker-compose -f docker-compose.optimized.yml run --rm \
  hermes-injector \
  sh -c "sleep 2 && /app/battle_test --host hermes-server:9999 --tokens $TOKENS --rate $RATE"

# Wait for completion
echo "⏳ Waiting for test completion..."
sleep 10

# Show logs
echo ""
echo "📊 Server Logs:"
echo "==============="
docker-compose -f docker-compose.optimized.yml logs hermes-server | tail -20

echo ""
echo "📊 Subscriber Logs:"
echo "==================="
docker logs hermes-subscriber-opt-test | tail -30

# Cleanup
echo ""
echo "🧹 Cleaning up..."
docker-compose -f docker-compose.optimized.yml down
docker rm -f hermes-subscriber-opt-test 2>/dev/null || true

echo ""
echo "✅ Optimized benchmark complete!"
echo ""
echo "💡 Tips for even better performance:"
echo "  - Ensure Docker has 4+ CPU cores allocated"
echo "  - Close other applications to reduce CPU contention"
echo "  - Run multiple times and take the best result"
echo "  - For production: Deploy to native Linux with full tuning"

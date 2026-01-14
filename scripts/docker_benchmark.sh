#!/bin/bash
# Docker Benchmark Script for Hermes

set -e

TOKENS=${1:-1000}
RATE=${2:-200}
DURATION=${3:-60}

echo "🐳 Hermes Docker Benchmark"
echo "=========================="
echo "Tokens: $TOKENS"
echo "Rate: $RATE msg/sec"
echo "Duration: $DURATION sec"
echo ""

# Build images
echo "📦 Building Docker images..."
docker-compose build

# Start server
echo "🚀 Starting Hermes server..."
docker-compose up -d hermes-server

# Wait for server to be ready
echo "⏳ Waiting for server to be ready..."
sleep 3

# Start subscriber
echo "📡 Starting subscriber..."
docker-compose run -d --name hermes-subscriber-test \
  hermes-subscriber \
  /app/hermes_subscriber --host hermes-server:9090 --duration $DURATION

# Wait for subscriber to connect
sleep 2

# Run injector
echo "💉 Starting message injector..."
docker-compose run --rm \
  hermes-injector \
  /app/battle_test --host hermes-server:9090 --tokens $TOKENS --rate $RATE

# Wait for completion
echo "⏳ Waiting for test completion..."
sleep 5

# Show logs
echo ""
echo "📊 Server Logs:"
echo "==============="
docker-compose logs hermes-server | tail -20

echo ""
echo "📊 Subscriber Logs:"
echo "==================="
docker logs hermes-subscriber-test | tail -30

# Cleanup
echo ""
echo "🧹 Cleaning up..."
docker-compose down
docker rm -f hermes-subscriber-test 2>/dev/null || true

echo ""
echo "✅ Benchmark complete!"

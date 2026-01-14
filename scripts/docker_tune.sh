#!/bin/bash
# Docker Runtime Tuning Script
# Apply kernel tuning inside container (requires --privileged or specific capabilities)

echo "🔧 Applying Docker runtime tuning..."

# Network tuning (if we have permission)
if [ -w /proc/sys/net/core/rmem_max ]; then
    echo "  📡 Tuning network buffers..."
    echo 268435456 > /proc/sys/net/core/rmem_max 2>/dev/null || true
    echo 268435456 > /proc/sys/net/core/wmem_max 2>/dev/null || true
    echo 268435456 > /proc/sys/net/core/rmem_default 2>/dev/null || true
    echo 268435456 > /proc/sys/net/core/wmem_default 2>/dev/null || true
    echo 10000 > /proc/sys/net/core/netdev_max_backlog 2>/dev/null || true
fi

# TCP tuning
if [ -w /proc/sys/net/ipv4/tcp_rmem ]; then
    echo "  🌐 Tuning TCP..."
    echo "4096 87380 268435456" > /proc/sys/net/ipv4/tcp_rmem 2>/dev/null || true
    echo "4096 65536 268435456" > /proc/sys/net/ipv4/tcp_wmem 2>/dev/null || true
    echo 1 > /proc/sys/net/ipv4/tcp_low_latency 2>/dev/null || true
fi

# VM tuning
if [ -w /proc/sys/vm/swappiness ]; then
    echo "  💾 Tuning VM..."
    echo 0 > /proc/sys/vm/swappiness 2>/dev/null || true
fi

echo "  ✅ Tuning applied (some may require --privileged mode)"
echo ""

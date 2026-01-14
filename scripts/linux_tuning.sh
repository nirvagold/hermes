#!/bin/bash
#
# Hermes Linux Performance Tuning Script
# ======================================
# Script untuk mengoptimalkan Linux agar mencapai P99 <50μs latency
#
# PERINGATAN: Script ini memerlukan akses root dan akan mengubah
# konfigurasi sistem. Gunakan dengan hati-hati di production!
#
# Usage:
#   sudo ./linux_tuning.sh setup    # Apply all optimizations
#   sudo ./linux_tuning.sh reset    # Revert to defaults
#   sudo ./linux_tuning.sh status   # Check current settings
#   sudo ./linux_tuning.sh bench    # Run benchmark with optimal settings

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration - Adjust these based on your CPU
HERMES_SERVER_CORE=0      # CPU core for server
HERMES_INJECTOR_CORE=1    # CPU core for injector/publisher
HERMES_SUBSCRIBER_CORE=2  # CPU core for subscriber
ISOLATED_CORES="0,1,2"    # Cores to isolate from scheduler

print_header() {
    echo -e "${BLUE}================================================${NC}"
    echo -e "${BLUE}  Hermes Linux Performance Tuning${NC}"
    echo -e "${BLUE}================================================${NC}"
    echo ""
}

check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}Error: This script must be run as root${NC}"
        echo "Usage: sudo $0 <command>"
        exit 1
    fi
}

# ============================================
# CPU Isolation & Frequency Tuning
# ============================================

setup_cpu_isolation() {
    echo -e "${YELLOW}[1/6] Setting up CPU isolation...${NC}"
    
    # Check if isolcpus is already in GRUB
    if grep -q "isolcpus" /etc/default/grub; then
        echo "  isolcpus already configured in GRUB"
    else
        echo "  Adding isolcpus to GRUB config..."
        echo ""
        echo -e "${YELLOW}  MANUAL STEP REQUIRED:${NC}"
        echo "  Add the following to GRUB_CMDLINE_LINUX in /etc/default/grub:"
        echo ""
        echo "    isolcpus=${ISOLATED_CORES} nohz_full=${ISOLATED_CORES} rcu_nocbs=${ISOLATED_CORES}"
        echo ""
        echo "  Then run: sudo update-grub && sudo reboot"
        echo ""
    fi
    
    # Disable CPU frequency scaling (set to performance mode)
    echo "  Setting CPU governor to 'performance'..."
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        if [ -f "$cpu" ]; then
            echo "performance" > "$cpu" 2>/dev/null || true
        fi
    done
    
    # Disable turbo boost for consistent latency
    if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
        echo "  Disabling Intel Turbo Boost..."
        echo 1 > /sys/devices/system/cpu/intel_pstate/no_turbo
    fi
    
    if [ -f /sys/devices/system/cpu/cpufreq/boost ]; then
        echo "  Disabling AMD Boost..."
        echo 0 > /sys/devices/system/cpu/cpufreq/boost
    fi
    
    echo -e "${GREEN}  ✓ CPU isolation configured${NC}"
}

# ============================================
# Network Stack Tuning
# ============================================

setup_network_tuning() {
    echo -e "${YELLOW}[2/6] Tuning network stack...${NC}"
    
    # Increase socket buffer sizes
    sysctl -w net.core.rmem_max=16777216 > /dev/null
    sysctl -w net.core.wmem_max=16777216 > /dev/null
    sysctl -w net.core.rmem_default=1048576 > /dev/null
    sysctl -w net.core.wmem_default=1048576 > /dev/null
    
    # TCP buffer sizes
    sysctl -w net.ipv4.tcp_rmem="4096 1048576 16777216" > /dev/null
    sysctl -w net.ipv4.tcp_wmem="4096 1048576 16777216" > /dev/null
    
    # Reduce TCP latency
    sysctl -w net.ipv4.tcp_low_latency=1 > /dev/null
    sysctl -w net.ipv4.tcp_fastopen=3 > /dev/null
    
    # Disable TCP slow start after idle
    sysctl -w net.ipv4.tcp_slow_start_after_idle=0 > /dev/null
    
    # Increase backlog
    sysctl -w net.core.somaxconn=65535 > /dev/null
    sysctl -w net.core.netdev_max_backlog=65535 > /dev/null
    
    # Disable timestamps for lower overhead
    sysctl -w net.ipv4.tcp_timestamps=0 > /dev/null
    
    echo -e "${GREEN}  ✓ Network stack tuned${NC}"
}

# ============================================
# Memory & Scheduler Tuning
# ============================================

setup_memory_tuning() {
    echo -e "${YELLOW}[3/6] Tuning memory subsystem...${NC}"
    
    # Disable swap (critical for low latency)
    swapoff -a 2>/dev/null || true
    
    # Reduce swappiness
    sysctl -w vm.swappiness=0 > /dev/null
    
    # Disable transparent huge pages (can cause latency spikes)
    if [ -f /sys/kernel/mm/transparent_hugepage/enabled ]; then
        echo never > /sys/kernel/mm/transparent_hugepage/enabled
    fi
    if [ -f /sys/kernel/mm/transparent_hugepage/defrag ]; then
        echo never > /sys/kernel/mm/transparent_hugepage/defrag
    fi
    
    # Lock memory to prevent paging
    sysctl -w vm.zone_reclaim_mode=0 > /dev/null
    
    # Increase max locked memory
    ulimit -l unlimited 2>/dev/null || true
    
    echo -e "${GREEN}  ✓ Memory subsystem tuned${NC}"
}

setup_scheduler_tuning() {
    echo -e "${YELLOW}[4/6] Tuning scheduler...${NC}"
    
    # Reduce scheduler latency
    sysctl -w kernel.sched_min_granularity_ns=100000 > /dev/null 2>&1 || true
    sysctl -w kernel.sched_wakeup_granularity_ns=25000 > /dev/null 2>&1 || true
    sysctl -w kernel.sched_migration_cost_ns=500000 > /dev/null 2>&1 || true
    
    # Disable kernel watchdog (can cause latency spikes)
    sysctl -w kernel.watchdog=0 > /dev/null 2>&1 || true
    sysctl -w kernel.nmi_watchdog=0 > /dev/null 2>&1 || true
    
    # Increase timer frequency (if available)
    # Note: This requires kernel recompile with CONFIG_HZ=1000
    
    echo -e "${GREEN}  ✓ Scheduler tuned${NC}"
}

# ============================================
# IRQ Affinity
# ============================================

setup_irq_affinity() {
    echo -e "${YELLOW}[5/6] Setting IRQ affinity...${NC}"
    
    # Move all IRQs away from isolated cores
    # This prevents hardware interrupts from disturbing Hermes
    
    # Get non-isolated cores mask (assuming 4+ cores, use cores 3+)
    NON_ISOLATED_MASK="f8"  # Cores 3-7 (adjust based on your CPU)
    
    for irq in /proc/irq/*/smp_affinity; do
        if [ -f "$irq" ]; then
            echo "$NON_ISOLATED_MASK" > "$irq" 2>/dev/null || true
        fi
    done
    
    echo -e "${GREEN}  ✓ IRQ affinity configured${NC}"
}

# ============================================
# Real-time Priority Setup
# ============================================

setup_realtime() {
    echo -e "${YELLOW}[6/6] Setting up real-time capabilities...${NC}"
    
    # Allow users to set real-time priority
    if ! grep -q "rtprio" /etc/security/limits.conf; then
        echo "* soft rtprio 99" >> /etc/security/limits.conf
        echo "* hard rtprio 99" >> /etc/security/limits.conf
        echo "* soft memlock unlimited" >> /etc/security/limits.conf
        echo "* hard memlock unlimited" >> /etc/security/limits.conf
    fi
    
    echo -e "${GREEN}  ✓ Real-time capabilities configured${NC}"
}

# ============================================
# Status Check
# ============================================

check_status() {
    echo -e "${YELLOW}Current System Status:${NC}"
    echo ""
    
    # CPU Governor
    echo "CPU Governor:"
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        if [ -f "$cpu" ]; then
            core=$(echo "$cpu" | grep -o 'cpu[0-9]*')
            gov=$(cat "$cpu")
            echo "  $core: $gov"
        fi
    done
    echo ""
    
    # Turbo Boost
    echo "Turbo Boost:"
    if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
        val=$(cat /sys/devices/system/cpu/intel_pstate/no_turbo)
        [ "$val" = "1" ] && echo "  Intel: Disabled" || echo "  Intel: Enabled"
    fi
    echo ""
    
    # Network settings
    echo "Network Buffer Sizes:"
    echo "  rmem_max: $(sysctl -n net.core.rmem_max)"
    echo "  wmem_max: $(sysctl -n net.core.wmem_max)"
    echo "  tcp_low_latency: $(sysctl -n net.ipv4.tcp_low_latency)"
    echo ""
    
    # Memory
    echo "Memory Settings:"
    echo "  swappiness: $(sysctl -n vm.swappiness)"
    if [ -f /sys/kernel/mm/transparent_hugepage/enabled ]; then
        echo "  THP: $(cat /sys/kernel/mm/transparent_hugepage/enabled)"
    fi
    echo ""
    
    # Check if isolcpus is active
    echo "CPU Isolation:"
    if grep -q "isolcpus" /proc/cmdline; then
        echo -e "  ${GREEN}isolcpus is ACTIVE${NC}"
        grep -o 'isolcpus=[^ ]*' /proc/cmdline
    else
        echo -e "  ${YELLOW}isolcpus is NOT active (requires reboot)${NC}"
    fi
}

# ============================================
# Benchmark Runner
# ============================================

run_benchmark() {
    echo -e "${YELLOW}Running Hermes Benchmark with Optimal Settings...${NC}"
    echo ""
    
    # Check if binaries exist
    if [ ! -f "./target/release/hermes_server" ]; then
        echo "Building Hermes..."
        cargo build --release
    fi
    
    # Kill any existing instances
    pkill -f hermes_server 2>/dev/null || true
    pkill -f hermes_subscriber 2>/dev/null || true
    sleep 1
    
    echo "Starting Hermes Server on CPU $HERMES_SERVER_CORE..."
    taskset -c $HERMES_SERVER_CORE chrt -f 99 ./target/release/hermes_server &
    SERVER_PID=$!
    sleep 2
    
    echo "Starting Hermes Subscriber on CPU $HERMES_SUBSCRIBER_CORE..."
    taskset -c $HERMES_SUBSCRIBER_CORE chrt -f 98 ./target/release/hermes_subscriber --duration 30 &
    SUBSCRIBER_PID=$!
    sleep 2
    
    echo "Starting Battle Test Injector on CPU $HERMES_INJECTOR_CORE..."
    taskset -c $HERMES_INJECTOR_CORE chrt -f 97 ./target/release/examples/battle_test --tokens 1000 --rate 500
    
    # Wait for subscriber to finish
    wait $SUBSCRIBER_PID 2>/dev/null || true
    
    # Cleanup
    kill $SERVER_PID 2>/dev/null || true
    
    echo ""
    echo -e "${GREEN}Benchmark complete!${NC}"
}

# ============================================
# Reset to Defaults
# ============================================

reset_settings() {
    echo -e "${YELLOW}Resetting to default settings...${NC}"
    
    # Reset CPU governor
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        if [ -f "$cpu" ]; then
            echo "ondemand" > "$cpu" 2>/dev/null || echo "powersave" > "$cpu" 2>/dev/null || true
        fi
    done
    
    # Re-enable turbo
    if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
        echo 0 > /sys/devices/system/cpu/intel_pstate/no_turbo
    fi
    
    # Reset network
    sysctl -w net.ipv4.tcp_low_latency=0 > /dev/null
    sysctl -w net.ipv4.tcp_timestamps=1 > /dev/null
    
    # Reset memory
    sysctl -w vm.swappiness=60 > /dev/null
    if [ -f /sys/kernel/mm/transparent_hugepage/enabled ]; then
        echo madvise > /sys/kernel/mm/transparent_hugepage/enabled
    fi
    
    echo -e "${GREEN}Settings reset to defaults${NC}"
    echo -e "${YELLOW}Note: isolcpus requires editing GRUB and rebooting to remove${NC}"
}

# ============================================
# Main
# ============================================

print_header

case "${1:-}" in
    setup)
        check_root
        setup_cpu_isolation
        setup_network_tuning
        setup_memory_tuning
        setup_scheduler_tuning
        setup_irq_affinity
        setup_realtime
        echo ""
        echo -e "${GREEN}================================================${NC}"
        echo -e "${GREEN}  All optimizations applied!${NC}"
        echo -e "${GREEN}================================================${NC}"
        echo ""
        echo "Next steps:"
        echo "  1. Add isolcpus to GRUB (see instructions above)"
        echo "  2. Reboot the system"
        echo "  3. Run: sudo ./linux_tuning.sh bench"
        ;;
    reset)
        check_root
        reset_settings
        ;;
    status)
        check_status
        ;;
    bench)
        check_root
        run_benchmark
        ;;
    *)
        echo "Usage: sudo $0 {setup|reset|status|bench}"
        echo ""
        echo "Commands:"
        echo "  setup   - Apply all performance optimizations"
        echo "  reset   - Revert to default settings"
        echo "  status  - Show current system configuration"
        echo "  bench   - Run benchmark with CPU pinning & RT priority"
        exit 1
        ;;
esac

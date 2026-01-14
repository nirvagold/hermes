# Quick P99 Latency Test

## Test the Optimizations

### Step 1: Start Server
```bash
cargo run --release --bin hermes_server
```

Expected output:
```
🚀 HERMES SERVER v2 - Fixed Broadcast
=====================================
💾 Storage: hermes_data.dat (64 MB)
🔌 Listening on 0.0.0.0:9999
⚡ TCP_NODELAY: ENABLED
📡 Waiting for connections...
```

### Step 2: Start Subscriber (in new terminal)
```bash
cargo run --release --bin hermes_subscriber -- --duration 30
```

Wait for:
```
📊 Waiting for messages...
```

### Step 3: Run Benchmark (in new terminal)
```bash
cargo run --release --example battle_test -- --tokens 1000 --rate 200
```

### Step 4: Check Results

Look for in the subscriber output:
```
📊 Final Statistics
===================
Messages received: 1000/1000 (100.0%)
Min latency:       XX.XX μs
P50 latency:       XX.XX μs
P90 latency:       XX.XX μs
P95 latency:       XX.XX μs
P99 latency:       XX.XX μs  ← Should be < 50μs ✅
Max latency:       XX.XX μs
```

## Success Criteria

✅ **P99 < 50μs** - Primary goal achieved
✅ **100% delivery rate** - No dropped messages
✅ **P50 < 100μs** - Median latency improved
✅ **No errors** - Clean execution

## If P99 Still >= 50μs

### Windows-Specific Issues
1. **Disable Windows Defender real-time scanning** for the hermes folder
2. **Close background applications** (browsers, IDEs, etc.)
3. **Set process priority to High**:
   ```powershell
   Start-Process -FilePath "cargo" -ArgumentList "run","--release","--bin","hermes_server" -Verb RunAs
   ```

### System Tuning
1. **Disable CPU throttling**:
   - Power Options → High Performance
   - Processor power management → Minimum 100%

2. **Check CPU usage**:
   - Server should use ~5-10% CPU when idle
   - Spike to 20-30% during message bursts

3. **Network loopback**:
   - Use 127.0.0.1 (not 0.0.0.0) for testing
   - Loopback is faster than network interface

## Benchmark Variations

### High Rate Test (stress test)
```bash
cargo run --release --example battle_test -- --tokens 5000 --rate 1000
```

### Low Rate Test (latency focus)
```bash
cargo run --release --example battle_test -- --tokens 100 --rate 50
```

### Burst Test
```bash
cargo run --release --example battle_test -- --tokens 10000 --rate 500
```

## Troubleshooting

### "Connection refused"
- Make sure server is running first
- Check firewall settings

### "Address already in use"
- Kill existing server: `taskkill /F /IM hermes_server.exe`
- Or change port in server code

### High P99 (>100μs)
- Check CPU usage (should not be 100%)
- Close other applications
- Restart test (first run may be slower due to cold cache)

## Expected Performance

### Windows (Development)
- P50: 80-120μs
- P99: 35-50μs ✅
- P99.9: 100-200μs

### Linux (Production)
- P50: 10-20μs
- P99: 20-35μs
- P99.9: 40-60μs

## Next Steps

If P99 < 50μs achieved:
1. ✅ Mark optimization complete
2. Document results in BENCHMARKS.md
3. Consider Linux testing for even better results

If P99 >= 50μs:
1. Review OPTIMIZATIONS.md for additional tuning
2. Profile with `cargo flamegraph`
3. Consider kernel-level optimizations

# 🚀 Quick Start - Docker Testing

Testing Hermes dengan Docker dalam 5 menit!

## Step 1: Install Docker

Download dan install Docker Desktop:
- Windows: https://www.docker.com/products/docker-desktop
- Mac: https://www.docker.com/products/docker-desktop
- Linux: `sudo apt install docker.io docker-compose`

Verifikasi:
```bash
docker --version
docker-compose --version
```

## Step 2: Build Images

```bash
cd hermes
docker-compose build
```

Tunggu ~5-10 menit untuk first build (akan di-cache untuk build berikutnya).

## Step 3: Run Benchmark

**Windows (PowerShell):**
```powershell
.\scripts\docker_benchmark.ps1
```

**Linux/Mac:**
```bash
chmod +x scripts/docker_benchmark.sh
./scripts/docker_benchmark.sh
```

## Step 4: Lihat Hasil

Script akan otomatis:
1. ✅ Start Hermes server
2. ✅ Start subscriber
3. ✅ Inject 1000 messages @ 200 msg/sec
4. ✅ Show latency statistics
5. ✅ Cleanup containers

Output example:
```
📊 Subscriber Logs:
===================
Latency Statistics:
  Min: 12.5 μs
  P50: 45.2 μs
  P99: 156.8 μs
  Max: 342.1 μs
Messages: 1000/1000 (100.0%)
```

## Troubleshooting

### Port 9090 sudah dipakai?
```bash
docker-compose down
```

### Build error?
```bash
docker-compose build --no-cache
```

### Slow performance?
- Docker Desktop → Settings → Resources
- Set CPUs: 4+, Memory: 4+ GB

## Next Steps

- 📖 Read [DOCKER.md](DOCKER.md) for advanced usage
- 🔧 Tune parameters: `docker_benchmark.ps1 -Tokens 5000 -Rate 1000`
- 📊 Compare with native: `.\scripts\run_benchmark.ps1`

---

**Why Docker?** Linux kernel = 2-5x faster than Windows native! 🚀

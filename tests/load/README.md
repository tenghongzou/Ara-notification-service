# Load Testing Suite

K6 負載測試套件，用於測試 Ara Notification Service 的效能與穩定性。

## 目錄

- [安裝需求](#安裝需求)
- [測試腳本](#測試腳本)
- [快速開始](#快速開始)
- [測試場景](#測試場景)
- [環境變數](#環境變數)
- [效能指標](#效能指標)
- [結果解讀](#結果解讀)
- [CI/CD 整合](#cicd-整合)

## 安裝需求

### K6 安裝

```bash
# macOS
brew install k6

# Windows (Chocolatey)
choco install k6

# Linux (Ubuntu/Debian)
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6

# Docker
docker pull grafana/k6
```

### 驗證安裝

```bash
k6 version
```

## 測試腳本

| 腳本 | 用途 | 說明 |
|------|------|------|
| `websocket.js` | WebSocket 連線測試 | 測試 WebSocket 連線建立、訂閱、訊息接收 |
| `http-api.js` | HTTP API 測試 | 測試通知發送 API 的吞吐量與延遲 |
| `batch-api.js` | 批次 API 測試 | 測試批次發送端點的效能 |
| `e2e-load.js` | 端對端測試 | 結合 WebSocket 與 HTTP，測試完整流程 |

## 快速開始

### 1. 啟動服務

```bash
# 確保服務已啟動
cargo run --release

# 或使用 Docker
docker-compose up -d
```

### 2. 準備認證資訊

```bash
# 設定環境變數
export JWT_TOKEN="your-jwt-token-here"
export API_KEY="your-api-key-here"
export HOST="localhost:8081"
```

### 3. 執行測試

```bash
# WebSocket 測試
k6 run websocket.js

# HTTP API 測試
k6 run http-api.js

# 批次 API 測試
k6 run batch-api.js

# 端對端測試
k6 run e2e-load.js
```

## 測試場景

### 預定義 Profile

| Profile | 連線數 | 請求/秒 | 持續時間 | 用途 |
|---------|--------|---------|----------|------|
| smoke | 10 | 10 | 30s | 快速驗證 |
| baseline | 100 | 50 | 2m | 基準效能 |
| medium | 500 | 100 | 3m | 尖峰時段模擬 |
| high | 1000 | 200 | 5m | 高流量測試 |
| stress | 2000 | 500 | 5m | 找出極限 |
| soak | 500 | 50 | 30m | 長時間穩定性 |
| spike | 100 | 1000 | 1m | 流量突增測試 |

### 使用 Profile

```bash
# 端對端測試使用不同 profile
k6 run --env PROFILE=baseline e2e-load.js
k6 run --env PROFILE=high e2e-load.js
k6 run --env PROFILE=stress e2e-load.js
```

### 自訂場景

透過命令列覆寫預設設定：

```bash
# 自訂 WebSocket 測試
k6 run \
  --env WS_HOST=production.example.com:443 \
  --env CHANNELS=orders,alerts \
  --env DURATION=300 \
  websocket.js

# 自訂 HTTP 測試
k6 run \
  --env API_HOST=production.example.com \
  --env SCENARIO=broadcast \
  http-api.js
```

## 環境變數

### 通用

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `HOST` | 伺服器地址 (host:port) | `localhost:8081` |
| `API_KEY` | HTTP API 認證金鑰 | - |
| `JWT_TOKEN` | WebSocket JWT Token | - |

### WebSocket 測試

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `WS_HOST` | WebSocket 伺服器地址 | `localhost:8081` |
| `CHANNELS` | 訂閱頻道 (逗號分隔) | `load-test` |
| `DURATION` | 連線持續時間 (秒) | `60` |

### HTTP API 測試

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `API_HOST` | API 伺服器地址 | `localhost:8081` |
| `SCENARIO` | 測試情境 (user/broadcast/channel/mixed) | `mixed` |

### 批次 API 測試

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `BATCH_SIZE` | 每批次通知數量 | `50` |

### 端對端測試

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `PROFILE` | 負載 profile (baseline/medium/high/stress) | `baseline` |

## 效能指標

### WebSocket 指標

| 指標 | 說明 | 目標值 |
|------|------|--------|
| `connection_success_rate` | 連線成功率 | > 95% |
| `message_latency_ms` | 訊息延遲 | p95 < 100ms |
| `ws_connections_failed` | 失敗連線數 | < 50 |

### HTTP API 指標

| 指標 | 說明 | 目標值 |
|------|------|--------|
| `request_success_rate` | 請求成功率 | > 99% |
| `request_latency_ms` | 請求延遲 | p95 < 50ms |
| `http_req_duration` | K6 內建延遲 | p95 < 100ms |

### 批次 API 指標

| 指標 | 說明 | 目標值 |
|------|------|--------|
| `batch_success_rate` | 批次成功率 | > 98% |
| `batch_latency_ms` | 批次延遲 | p95 < 200ms |

### 端對端指標

| 指標 | 說明 | 目標值 |
|------|------|--------|
| `e2e_latency_ms` | 端對端延遲 | p95 < 200ms |
| `overall_success_rate` | 整體成功率 | > 95% |

## 結果解讀

### 成功輸出範例

```
     ✓ status is 200
     ✓ response has notification_id

     checks.........................: 100.00% ✓ 5000   ✗ 0
     data_received..................: 1.2 MB  20 kB/s
     data_sent......................: 850 kB  14 kB/s
     http_req_duration..............: avg=12.5ms min=5ms med=10ms max=150ms p(90)=25ms p(95)=35ms
     http_reqs......................: 5000    83.33/s
     iteration_duration.............: avg=15ms   min=6ms med=12ms max=160ms p(90)=30ms p(95)=40ms
     iterations.....................: 5000    83.33/s
     request_success_rate...........: 100.00% ✓ 5000   ✗ 0
```

### 失敗指標處理

當 threshold 未達標時：

```
     ✗ request_latency_ms
      ↳  95% response time below 50ms
         threshold: p(95)<50 failed
         actual: p(95)=78ms

ERRO[0125] some thresholds have failed
```

**解決方案：**
1. 檢查伺服器資源使用率
2. 調整連線數/請求率
3. 檢視 `/stats` 端點的瓶頸指標
4. 使用 Prometheus/Grafana 進行深入分析

## CI/CD 整合

### GitHub Actions 範例

```yaml
name: Load Test

on:
  schedule:
    - cron: '0 2 * * *'  # 每日 02:00
  workflow_dispatch:

jobs:
  load-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Start services
        run: docker-compose up -d

      - name: Wait for services
        run: sleep 10

      - name: Run K6 Load Test
        uses: grafana/k6-action@v0.3.1
        with:
          filename: tests/load/http-api.js
        env:
          API_HOST: localhost:8081
          API_KEY: ${{ secrets.API_KEY }}

      - name: Upload results
        uses: actions/upload-artifact@v4
        with:
          name: k6-results
          path: results/
```

### Docker 執行

```bash
# 使用 Docker 執行 K6
docker run --rm -i \
  -e API_HOST=host.docker.internal:8081 \
  -e API_KEY=your-key \
  -v $(pwd)/tests/load:/scripts \
  grafana/k6 run /scripts/http-api.js
```

### 輸出報告

```bash
# JSON 輸出
k6 run --out json=results.json http-api.js

# CSV 輸出
k6 run --out csv=results.csv http-api.js

# InfluxDB 輸出 (配合 Grafana)
k6 run --out influxdb=http://localhost:8086/k6 http-api.js
```

## 目錄結構

```
tests/load/
├── README.md           # 本文檔
├── config.js           # 共享配置
├── websocket.js        # WebSocket 負載測試
├── http-api.js         # HTTP API 負載測試
├── batch-api.js        # 批次 API 負載測試
├── e2e-load.js         # 端對端負載測試
└── utils/
    └── jwt-generator.js # JWT 生成工具
```

## 注意事項

1. **生產環境測試**：在生產環境執行負載測試前，請確保已獲得授權並做好流量隔離
2. **資源監控**：測試期間應監控伺服器 CPU、記憶體、網路使用率
3. **漸進式測試**：從 smoke 測試開始，逐步增加負載
4. **結果基準**：建立基準測試結果，追蹤效能變化
5. **清理資料**：測試後清理測試產生的連線和訊息

## 參考資源

- [K6 官方文檔](https://k6.io/docs/)
- [K6 WebSocket 指南](https://k6.io/docs/javascript-api/k6-ws/)
- [Grafana K6 Dashboard](https://grafana.com/grafana/dashboards/2587-k6-load-testing-results/)

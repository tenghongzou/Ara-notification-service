# Ara Notification Service 技術文件

歡迎閱讀 Ara Notification Service 的技術文件。本文件涵蓋系統架構、安裝部署、API 參考、開發指南與進階功能。

---

## 文件目錄

### 核心文件

| 文件 | 說明 |
|------|------|
| [系統架構](./01-architecture.md) | 技術棧、模組設計、資料流、設計模式 |
| [安裝與部署](./02-installation.md) | 環境需求、本地開發、Docker 部署、生產配置 |
| [API 參考](./03-api-reference.md) | REST API、WebSocket 協定、Redis Pub/Sub |
| [開發指南](./04-development-guide.md) | 程式碼規範、模組擴展、測試指南 |
| [進階功能](./05-advanced-features.md) | 離線佇列、ACK 追蹤、多租戶、叢集模式 |
| [可觀測性](./06-observability.md) | Prometheus 監控、OpenTelemetry 追蹤、日誌 |
| [單機商業部署與安全](./07-single-node-commercial-secure.md) | 單機商業部署、多租戶、安全範本 |

---

## 快速導覽

### 我是新手，從哪裡開始？

1. 閱讀 [系統架構](./01-architecture.md) 了解整體設計
2. 參考 [安裝與部署](./02-installation.md) 設定開發環境
3. 查閱 [API 參考](./03-api-reference.md) 了解如何發送通知

### 我要整合通知服務到後端？

1. 閱讀 [API 參考](./03-api-reference.md) 了解 HTTP API 與 Redis Pub/Sub
2. 參考主專案 README 的整合範例
3. 查看 [進階功能](./05-advanced-features.md) 了解模板系統

### 我要開發前端即時通知？

1. 閱讀 [API 參考](./03-api-reference.md) 的 WebSocket/SSE 章節
2. 參考主專案 README 的 JavaScript 客戶端範例
3. 了解頻道訂閱與 ACK 確認機制

---

## 專案特色

### 技術亮點

- **Rust + Tokio**：高效能非同步執行環境，支援 10,000+ 並發連線
- **雙協定支援**：WebSocket 雙向通訊 + SSE 單向推送（防火牆友好）
- **三索引設計**：DashMap 實現 O(1) 連線查詢
- **熔斷器模式**：Redis 故障隔離，服務持續可用
- **SmallVec 優化**：減少記憶體分配開銷

### 功能模組

| 模組 | 說明 |
|------|------|
| WebSocket Handler | 雙向即時通訊、心跳檢測、自動重連 |
| SSE Handler | 單向推送、防火牆友好 |
| Connection Manager | 三索引連線管理、多裝置支援 |
| Notification Dispatcher | 多種發送模式（點對點、廣播、頻道） |
| Template Store | 通知模板、變數替換 |
| Message Queue | 離線訊息佇列、重連重播 |
| Rate Limiter | Token Bucket 限流 |
| ACK Tracker | 送達確認追蹤 |
| Tenant Manager | 多租戶隔離 |
| Cluster Router | 分散式叢集路由 |

### 訊息模式

| 模式 | 說明 | 使用場景 |
|------|------|----------|
| User | 發送給特定使用者所有裝置 | 個人通知、私訊 |
| Users | 發送給多個使用者 | 群組通知 |
| Broadcast | 發送給所有連線使用者 | 系統公告 |
| Channel | 發送給頻道訂閱者 | 訂單狀態更新 |
| Channels | 發送給多個頻道 | 跨類別通知 |

---

## 相關連結

- [English Documentation](../en/README.md)
- [專案 README](../../README.md)
- [API 規格 (原始)](../API.md)
- [架構文件 (原始)](../ARCHITECTURE.md)
- [變更記錄](../../CHANGELOG.md)

/**
 * K6 End-to-End Load Test for Ara Notification Service
 *
 * Combines WebSocket connections and HTTP API load testing to simulate
 * real-world usage patterns. WebSocket clients receive notifications
 * sent via HTTP API.
 *
 * Usage:
 *   k6 run --env HOST=localhost:8081 --env JWT_TOKEN=<token> --env API_KEY=<key> e2e-load.js
 *
 * Environment Variables:
 *   HOST       - Server host:port (default: localhost:8081)
 *   JWT_TOKEN  - JWT token for WebSocket authentication
 *   API_KEY    - API key for HTTP authentication
 *   PROFILE    - Load profile: baseline, medium, high, stress (default: baseline)
 */

import ws from 'k6/ws';
import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Trend, Rate, Gauge } from 'k6/metrics';
import { randomString, randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Custom metrics - WebSocket
const wsConnections = new Counter('ws_connections_total');
const wsConnectionsFailed = new Counter('ws_connections_failed');
const wsMessagesReceived = new Counter('ws_messages_received');
const notificationsReceived = new Counter('notifications_received');
const e2eLatency = new Trend('e2e_latency_ms');
const activeConnections = new Gauge('active_connections');

// Custom metrics - HTTP
const httpRequests = new Counter('http_requests_total');
const httpFailed = new Counter('http_requests_failed');
const httpLatency = new Trend('http_latency_ms');

// Custom metrics - Combined
const overallSuccess = new Rate('overall_success_rate');

// Test configuration
const HOST = __ENV.HOST || 'localhost:8081';
const JWT_TOKEN = __ENV.JWT_TOKEN || '';
const API_KEY = __ENV.API_KEY || '';
const PROFILE = __ENV.PROFILE || 'baseline';

const BASE_URL = `http://${HOST}`;
const WS_URL = `ws://${HOST}/ws`;

// Load profiles
const PROFILES = {
  baseline: {
    wsVUs: 100,
    httpRate: 50,
    duration: '2m',
    description: 'Baseline: 100 connections, 50 req/s',
  },
  medium: {
    wsVUs: 500,
    httpRate: 100,
    duration: '3m',
    description: 'Medium: 500 connections, 100 req/s',
  },
  high: {
    wsVUs: 1000,
    httpRate: 200,
    duration: '5m',
    description: 'High: 1000 connections, 200 req/s',
  },
  stress: {
    wsVUs: 2000,
    httpRate: 500,
    duration: '5m',
    description: 'Stress: 2000 connections, 500 req/s',
  },
};

const profile = PROFILES[PROFILE] || PROFILES.baseline;

// Common headers
const headers = {
  'Content-Type': 'application/json',
  'X-API-Key': API_KEY,
};

// Test channel for e2e testing
const E2E_CHANNEL = 'e2e-load-test';

// Test scenarios
export const options = {
  scenarios: {
    // WebSocket connections scenario
    websocket_clients: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: profile.wsVUs / 2 },
        { duration: '30s', target: profile.wsVUs },
        { duration: profile.duration, target: profile.wsVUs },
        { duration: '30s', target: 0 },
      ],
      exec: 'websocketClient',
      gracefulRampDown: '30s',
    },

    // HTTP API scenario (starts after WebSocket clients are connected)
    http_sender: {
      executor: 'ramping-rate',
      startRate: 0,
      timeUnit: '1s',
      preAllocatedVUs: 50,
      maxVUs: 200,
      stages: [
        { duration: '1m', target: 0 },                    // Wait for WS to connect
        { duration: '30s', target: profile.httpRate },
        { duration: profile.duration, target: profile.httpRate },
        { duration: '30s', target: 0 },
      ],
      exec: 'httpSender',
    },
  },
  thresholds: {
    'overall_success_rate': ['rate>0.95'],
    'e2e_latency_ms': ['p(95)<200', 'p(99)<500'],
    'http_latency_ms': ['p(95)<50'],
    'ws_connections_failed': ['count<10'],
  },
};

// WebSocket client function
export function websocketClient() {
  const url = `${WS_URL}?token=${JWT_TOKEN}`;
  const clientId = randomString(8);

  const res = ws.connect(url, {}, function (socket) {
    socket.on('open', () => {
      wsConnections.add(1);
      activeConnections.add(1);

      // Subscribe to e2e test channel
      socket.send(JSON.stringify({
        type: 'Subscribe',
        payload: { channels: [E2E_CHANNEL] }
      }));
    });

    socket.on('message', (data) => {
      wsMessagesReceived.add(1);

      try {
        const msg = JSON.parse(data);

        if (msg.type === 'notification') {
          notificationsReceived.add(1);

          // Calculate e2e latency
          if (msg.event && msg.event.payload && msg.event.payload.sent_at) {
            const latency = Date.now() - msg.event.payload.sent_at;
            if (latency > 0 && latency < 30000) {
              e2eLatency.add(latency);
            }
          }

          overallSuccess.add(1);
        }
      } catch (e) {
        // Ignore parse errors
      }
    });

    socket.on('close', () => {
      activeConnections.add(-1);
    });

    // Send periodic pings
    socket.setInterval(() => {
      socket.send(JSON.stringify({ type: 'Ping' }));
    }, 25000);

    // Keep connection open
    socket.setTimeout(() => {
      socket.send(JSON.stringify({
        type: 'Unsubscribe',
        payload: { channels: [E2E_CHANNEL] }
      }));
      sleep(0.5);
      socket.close();
    }, 180000); // 3 minutes max
  });

  const connected = check(res, {
    'WS connected': (r) => r && r.status === 101,
  });

  if (!connected) {
    wsConnectionsFailed.add(1);
    overallSuccess.add(0);
  }
}

// HTTP sender function
export function httpSender() {
  // Send to e2e channel so WebSocket clients receive it
  const payload = {
    channel: E2E_CHANNEL,
    event_type: 'e2e.load.test',
    source: 'k6-e2e-test',
    payload: {
      message: `E2E test notification`,
      sent_at: Date.now(),
      sender_id: randomString(8),
    },
  };

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/channel`,
    JSON.stringify(payload),
    { headers, tags: { name: 'channel_notification' } }
  );

  httpRequests.add(1);
  httpLatency.add(res.timings.duration);

  const success = check(res, {
    'HTTP 200': (r) => r.status === 200,
  });

  if (!success) {
    httpFailed.add(1);
    overallSuccess.add(0);
  }

  sleep(Math.random() * 0.1);
}

// Setup function
export function setup() {
  console.log('='.repeat(70));
  console.log('Ara Notification Service - End-to-End Load Test');
  console.log('='.repeat(70));
  console.log(`Profile: ${PROFILE} - ${profile.description}`);
  console.log(`Target: ${HOST}`);
  console.log(`E2E Channel: ${E2E_CHANNEL}`);
  console.log('='.repeat(70));

  // Health check
  const healthRes = http.get(`${BASE_URL}/health`);
  if (healthRes.status !== 200) {
    console.error('FATAL: Health check failed');
    return { error: true };
  }

  // Verify WebSocket
  const wsRes = ws.connect(`${WS_URL}?token=${JWT_TOKEN}`, { timeout: '5s' }, (socket) => {
    socket.on('open', () => socket.close());
    socket.setTimeout(() => socket.close(), 2000);
  });

  if (!wsRes || wsRes.status !== 101) {
    console.error('FATAL: WebSocket connection test failed');
    return { error: true };
  }

  // Verify HTTP API
  const apiRes = http.post(
    `${BASE_URL}/api/v1/notifications/channel`,
    JSON.stringify({
      channel: E2E_CHANNEL,
      event_type: 'test',
      source: 'k6-setup',
      payload: {},
    }),
    { headers }
  );

  if (apiRes.status !== 200) {
    console.error('FATAL: HTTP API test failed');
    return { error: true };
  }

  console.log('Setup complete - all endpoints verified');
  return { error: false };
}

// Teardown function
export function teardown(data) {
  console.log('='.repeat(70));
  console.log('End-to-End Load Test Complete');
  console.log('='.repeat(70));

  if (data && data.error) {
    console.log('Test aborted due to setup failure');
  }
}

/**
 * K6 WebSocket Load Test for Ara Notification Service
 *
 * Tests WebSocket connection establishment, channel subscriptions,
 * and notification delivery under load.
 *
 * Usage:
 *   k6 run --env WS_HOST=localhost:8081 --env JWT_TOKEN=<token> websocket.js
 *
 * Environment Variables:
 *   WS_HOST    - WebSocket server host:port (default: localhost:8081)
 *   JWT_TOKEN  - JWT token for authentication
 *   CHANNELS   - Comma-separated channel names to subscribe (default: load-test)
 *   DURATION   - Connection duration in seconds (default: 60)
 */

import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Trend, Rate, Gauge } from 'k6/metrics';

// Custom metrics
const wsConnections = new Counter('ws_connections_total');
const wsConnectionsFailed = new Counter('ws_connections_failed');
const wsMessagesReceived = new Counter('ws_messages_received');
const wsMessagesSent = new Counter('ws_messages_sent');
const notificationsReceived = new Counter('notifications_received');
const messageLatency = new Trend('message_latency_ms');
const connectionDuration = new Trend('connection_duration_s');
const connectionSuccess = new Rate('connection_success_rate');
const activeConnections = new Gauge('active_connections');

// Test configuration
const WS_HOST = __ENV.WS_HOST || 'localhost:8081';
const JWT_TOKEN = __ENV.JWT_TOKEN || '';
const CHANNELS = (__ENV.CHANNELS || 'load-test').split(',');
const CONNECTION_DURATION = parseInt(__ENV.DURATION || '60', 10) * 1000;

// Test scenarios
export const options = {
  scenarios: {
    // Ramp up to target connections
    websocket_load: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 100 },    // Warm up
        { duration: '1m', target: 500 },     // Ramp to 500
        { duration: '2m', target: 1000 },    // Ramp to 1000
        { duration: '3m', target: 1000 },    // Hold at 1000
        { duration: '1m', target: 500 },     // Scale down
        { duration: '30s', target: 0 },      // Cool down
      ],
      gracefulRampDown: '30s',
    },
  },
  thresholds: {
    'connection_success_rate': ['rate>0.95'],           // 95% connection success
    'message_latency_ms': ['p(95)<100', 'p(99)<500'],   // 95th percentile < 100ms
    'ws_connections_failed': ['count<50'],              // Less than 50 failed connections
  },
};

// Validate configuration
if (!JWT_TOKEN) {
  console.error('ERROR: JWT_TOKEN environment variable is required');
  console.error('Usage: k6 run --env JWT_TOKEN=<token> websocket.js');
}

export default function () {
  const url = `ws://${WS_HOST}/ws?token=${JWT_TOKEN}`;
  const startTime = Date.now();

  const res = ws.connect(url, {}, function (socket) {
    let messagesReceived = 0;

    socket.on('open', () => {
      wsConnections.add(1);
      activeConnections.add(1);

      // Subscribe to channels
      const subscribeMsg = JSON.stringify({
        type: 'Subscribe',
        payload: { channels: CHANNELS }
      });
      socket.send(subscribeMsg);
      wsMessagesSent.add(1);
    });

    socket.on('message', (data) => {
      wsMessagesReceived.add(1);
      messagesReceived++;

      try {
        const msg = JSON.parse(data);

        switch (msg.type) {
          case 'notification':
            notificationsReceived.add(1);
            // Calculate latency if timestamp is available
            if (msg.event && msg.event.created_at) {
              const sentAt = new Date(msg.event.created_at).getTime();
              const latency = Date.now() - sentAt;
              if (latency > 0 && latency < 60000) { // Sanity check
                messageLatency.add(latency);
              }
            }
            break;

          case 'subscribed':
            // Successfully subscribed to channels
            break;

          case 'pong':
            // Heartbeat response
            break;

          case 'heartbeat':
            // Server heartbeat
            break;

          case 'error':
            console.warn(`Server error: ${msg.error?.message || JSON.stringify(msg)}`);
            break;
        }
      } catch (e) {
        console.warn(`Failed to parse message: ${e.message}`);
      }
    });

    socket.on('error', (e) => {
      console.error(`WebSocket error: ${e.message || e}`);
    });

    socket.on('close', () => {
      activeConnections.add(-1);
    });

    // Send periodic pings to keep connection alive
    socket.setInterval(() => {
      socket.send(JSON.stringify({ type: 'Ping' }));
      wsMessagesSent.add(1);
    }, 25000); // Every 25 seconds

    // Keep connection open for the specified duration
    socket.setTimeout(() => {
      // Unsubscribe before closing
      socket.send(JSON.stringify({
        type: 'Unsubscribe',
        payload: { channels: CHANNELS }
      }));
      wsMessagesSent.add(1);

      sleep(0.5);
      socket.close();
    }, CONNECTION_DURATION);
  });

  // Check connection result
  const connected = check(res, {
    'WebSocket connected': (r) => r && r.status === 101,
  });

  connectionSuccess.add(connected ? 1 : 0);

  if (!connected) {
    wsConnectionsFailed.add(1);
    console.warn(`Connection failed: status=${res ? res.status : 'null'}`);
  }

  // Record connection duration
  const duration = (Date.now() - startTime) / 1000;
  connectionDuration.add(duration);
}

// Setup function - runs once before the test
export function setup() {
  console.log('='.repeat(60));
  console.log('Ara Notification Service - WebSocket Load Test');
  console.log('='.repeat(60));
  console.log(`Target: ws://${WS_HOST}/ws`);
  console.log(`Channels: ${CHANNELS.join(', ')}`);
  console.log(`Connection Duration: ${CONNECTION_DURATION / 1000}s`);
  console.log('='.repeat(60));

  // Verify connectivity
  const url = `ws://${WS_HOST}/ws?token=${JWT_TOKEN}`;
  const res = ws.connect(url, { timeout: '5s' }, function (socket) {
    socket.on('open', () => {
      socket.close();
    });
    socket.setTimeout(() => socket.close(), 2000);
  });

  if (!res || res.status !== 101) {
    console.error('FATAL: Cannot connect to WebSocket server');
    console.error('Please verify the server is running and JWT_TOKEN is valid');
    return { error: true };
  }

  console.log('Connectivity check passed');
  return { error: false };
}

// Teardown function - runs once after the test
export function teardown(data) {
  console.log('='.repeat(60));
  console.log('Load Test Complete');
  console.log('='.repeat(60));

  if (data && data.error) {
    console.log('Test failed due to connectivity issues');
  }
}

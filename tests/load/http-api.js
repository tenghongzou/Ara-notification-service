/**
 * K6 HTTP API Load Test for Ara Notification Service
 *
 * Tests HTTP notification endpoints under load, including:
 * - Single user notifications
 * - Multi-user notifications
 * - Broadcast notifications
 * - Channel notifications
 *
 * Usage:
 *   k6 run --env API_HOST=localhost:8081 --env API_KEY=<key> http-api.js
 *
 * Environment Variables:
 *   API_HOST  - API server host:port (default: localhost:8081)
 *   API_KEY   - API key for authentication
 *   SCENARIO  - Test scenario: user, broadcast, channel, mixed (default: mixed)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend, Rate } from 'k6/metrics';
import { randomString, randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Custom metrics
const requestsTotal = new Counter('http_requests_total');
const requestsFailed = new Counter('http_requests_failed');
const notificationsSent = new Counter('notifications_sent');
const deliveriesTotal = new Counter('deliveries_total');
const requestLatency = new Trend('request_latency_ms');
const requestSuccess = new Rate('request_success_rate');

// Test configuration
const API_HOST = __ENV.API_HOST || 'localhost:8081';
const API_KEY = __ENV.API_KEY || '';
const SCENARIO = __ENV.SCENARIO || 'mixed';
const BASE_URL = `http://${API_HOST}`;

// Common headers
const headers = {
  'Content-Type': 'application/json',
  'X-API-Key': API_KEY,
};

// Test scenarios
export const options = {
  scenarios: {
    http_load: {
      executor: 'ramping-rate',
      startRate: 10,
      timeUnit: '1s',
      preAllocatedVUs: 50,
      maxVUs: 200,
      stages: [
        { duration: '30s', target: 50 },    // Warm up to 50 req/s
        { duration: '1m', target: 100 },    // Ramp to 100 req/s
        { duration: '2m', target: 200 },    // Ramp to 200 req/s
        { duration: '2m', target: 200 },    // Hold at 200 req/s
        { duration: '1m', target: 100 },    // Scale down
        { duration: '30s', target: 0 },     // Cool down
      ],
    },
  },
  thresholds: {
    'request_success_rate': ['rate>0.99'],           // 99% success rate
    'request_latency_ms': ['p(95)<50', 'p(99)<100'], // 95th percentile < 50ms
    'http_req_duration': ['p(95)<100'],              // K6 built-in latency
  },
};

// Generate random user ID
function randomUserId() {
  return `user-${randomString(8)}`;
}

// Generate random channel name
function randomChannel() {
  const channels = ['orders', 'alerts', 'updates', 'news', 'system'];
  return channels[randomIntBetween(0, channels.length - 1)];
}

// Generate notification payload
function generatePayload() {
  return {
    event_type: 'load.test',
    message: `Load test notification at ${new Date().toISOString()}`,
    test_id: randomString(16),
    timestamp: Date.now(),
  };
}

// Send notification to single user
function sendToUser() {
  const payload = {
    target_user_id: randomUserId(),
    event_type: 'load.test.user',
    source: 'k6-load-test',
    payload: generatePayload(),
  };

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/send`,
    JSON.stringify(payload),
    { headers, tags: { name: 'send_to_user' } }
  );

  return processResponse(res, 'send_to_user');
}

// Send notification to multiple users
function sendToUsers() {
  const userCount = randomIntBetween(2, 10);
  const userIds = Array.from({ length: userCount }, () => randomUserId());

  const payload = {
    target_user_ids: userIds,
    event_type: 'load.test.users',
    source: 'k6-load-test',
    payload: generatePayload(),
  };

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/send-to-users`,
    JSON.stringify(payload),
    { headers, tags: { name: 'send_to_users' } }
  );

  return processResponse(res, 'send_to_users');
}

// Send broadcast notification
function sendBroadcast() {
  const payload = {
    event_type: 'load.test.broadcast',
    source: 'k6-load-test',
    payload: generatePayload(),
  };

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/broadcast`,
    JSON.stringify(payload),
    { headers, tags: { name: 'broadcast' } }
  );

  return processResponse(res, 'broadcast');
}

// Send channel notification
function sendToChannel() {
  const payload = {
    channel: randomChannel(),
    event_type: 'load.test.channel',
    source: 'k6-load-test',
    payload: generatePayload(),
  };

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/channel`,
    JSON.stringify(payload),
    { headers, tags: { name: 'channel' } }
  );

  return processResponse(res, 'channel');
}

// Process response and update metrics
function processResponse(res, endpoint) {
  requestsTotal.add(1);
  requestLatency.add(res.timings.duration);

  const success = check(res, {
    'status is 200': (r) => r.status === 200,
    'response has notification_id': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.notification_id !== undefined;
      } catch {
        return false;
      }
    },
  });

  requestSuccess.add(success ? 1 : 0);

  if (success) {
    notificationsSent.add(1);
    try {
      const body = JSON.parse(res.body);
      if (body.delivered_to) {
        deliveriesTotal.add(body.delivered_to);
      }
    } catch {
      // Ignore parse errors
    }
  } else {
    requestsFailed.add(1);
    if (res.status !== 200) {
      console.warn(`${endpoint} failed: status=${res.status}, body=${res.body}`);
    }
  }

  return success;
}

// Main test function
export default function () {
  switch (SCENARIO) {
    case 'user':
      sendToUser();
      break;
    case 'users':
      sendToUsers();
      break;
    case 'broadcast':
      sendBroadcast();
      break;
    case 'channel':
      sendToChannel();
      break;
    case 'mixed':
    default:
      // Weighted distribution: 50% user, 20% users, 10% broadcast, 20% channel
      const rand = Math.random();
      if (rand < 0.5) {
        sendToUser();
      } else if (rand < 0.7) {
        sendToUsers();
      } else if (rand < 0.8) {
        sendBroadcast();
      } else {
        sendToChannel();
      }
      break;
  }

  // Small random sleep to avoid thundering herd
  sleep(Math.random() * 0.1);
}

// Setup function
export function setup() {
  console.log('='.repeat(60));
  console.log('Ara Notification Service - HTTP API Load Test');
  console.log('='.repeat(60));
  console.log(`Target: ${BASE_URL}`);
  console.log(`Scenario: ${SCENARIO}`);
  console.log('='.repeat(60));

  // Health check
  const healthRes = http.get(`${BASE_URL}/health`);
  if (healthRes.status !== 200) {
    console.error('FATAL: Health check failed');
    console.error(`Status: ${healthRes.status}, Body: ${healthRes.body}`);
    return { error: true };
  }

  // Verify API key (if required)
  if (API_KEY) {
    const testRes = http.post(
      `${BASE_URL}/api/v1/notifications/send`,
      JSON.stringify({
        target_user_id: 'test-user',
        event_type: 'test',
        source: 'k6-setup',
        payload: {},
      }),
      { headers }
    );

    if (testRes.status === 401) {
      console.error('FATAL: API key authentication failed');
      return { error: true };
    }
  }

  console.log('Setup complete');
  return { error: false };
}

// Teardown function
export function teardown(data) {
  console.log('='.repeat(60));
  console.log('HTTP API Load Test Complete');
  console.log('='.repeat(60));

  if (data && data.error) {
    console.log('Test failed due to setup issues');
  }
}

/**
 * K6 Batch API Load Test for Ara Notification Service
 *
 * Tests the batch notification endpoint under load.
 * Each request sends multiple notifications in a single API call.
 *
 * Usage:
 *   k6 run --env API_HOST=localhost:8081 --env API_KEY=<key> batch-api.js
 *
 * Environment Variables:
 *   API_HOST    - API server host:port (default: localhost:8081)
 *   API_KEY     - API key for authentication
 *   BATCH_SIZE  - Number of notifications per batch (default: 50)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend, Rate } from 'k6/metrics';
import { randomString, randomIntBetween } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Custom metrics
const batchRequests = new Counter('batch_requests_total');
const batchFailed = new Counter('batch_requests_failed');
const notificationsInBatch = new Counter('notifications_in_batch');
const batchLatency = new Trend('batch_latency_ms');
const batchSuccess = new Rate('batch_success_rate');
const notificationsSucceeded = new Counter('notifications_succeeded');
const notificationsFailed = new Counter('notifications_failed');
const deliveriesTotal = new Counter('deliveries_total');

// Test configuration
const API_HOST = __ENV.API_HOST || 'localhost:8081';
const API_KEY = __ENV.API_KEY || '';
const BATCH_SIZE = parseInt(__ENV.BATCH_SIZE || '50', 10);
const BASE_URL = `http://${API_HOST}`;

// Common headers
const headers = {
  'Content-Type': 'application/json',
  'X-API-Key': API_KEY,
};

// Test scenarios - Lower RPS due to batch nature
export const options = {
  scenarios: {
    batch_load: {
      executor: 'ramping-rate',
      startRate: 1,
      timeUnit: '1s',
      preAllocatedVUs: 20,
      maxVUs: 100,
      stages: [
        { duration: '30s', target: 5 },     // Warm up to 5 batch/s
        { duration: '1m', target: 10 },     // Ramp to 10 batch/s
        { duration: '2m', target: 20 },     // Ramp to 20 batch/s (1000 notif/s)
        { duration: '2m', target: 20 },     // Hold at 20 batch/s
        { duration: '1m', target: 10 },     // Scale down
        { duration: '30s', target: 0 },     // Cool down
      ],
    },
  },
  thresholds: {
    'batch_success_rate': ['rate>0.98'],              // 98% batch success
    'batch_latency_ms': ['p(95)<200', 'p(99)<500'],   // 95th percentile < 200ms
  },
};

// Target types with weights
const TARGET_TYPES = [
  { type: 'user', weight: 0.4 },
  { type: 'users', weight: 0.2 },
  { type: 'channel', weight: 0.3 },
  { type: 'broadcast', weight: 0.1 },
];

// Channel names for testing
const CHANNELS = ['orders', 'alerts', 'updates', 'news', 'system', 'marketing'];

// Generate random user ID
function randomUserId() {
  return `user-${randomString(8)}`;
}

// Generate random channel
function randomChannel() {
  return CHANNELS[randomIntBetween(0, CHANNELS.length - 1)];
}

// Select target type based on weights
function selectTargetType() {
  const rand = Math.random();
  let cumulative = 0;

  for (const target of TARGET_TYPES) {
    cumulative += target.weight;
    if (rand < cumulative) {
      return target.type;
    }
  }

  return 'user';
}

// Generate a single notification item for batch
function generateNotificationItem(index) {
  const targetType = selectTargetType();
  let target;

  switch (targetType) {
    case 'user':
      target = { type: 'user', value: randomUserId() };
      break;
    case 'users':
      const userCount = randomIntBetween(2, 5);
      const userIds = Array.from({ length: userCount }, () => randomUserId());
      target = { type: 'users', value: userIds };
      break;
    case 'channel':
      target = { type: 'channel', value: randomChannel() };
      break;
    case 'broadcast':
      target = { type: 'broadcast' };
      break;
    default:
      target = { type: 'user', value: randomUserId() };
  }

  return {
    target,
    event_type: `load.test.batch.${targetType}`,
    source: 'k6-batch-test',
    payload: {
      batch_index: index,
      message: `Batch notification ${index}`,
      timestamp: Date.now(),
      test_id: randomString(12),
    },
    priority: randomIntBetween(0, 2) === 0 ? 'High' : 'Normal',
  };
}

// Generate batch request
function generateBatchRequest(size) {
  const notifications = [];

  for (let i = 0; i < size; i++) {
    notifications.push(generateNotificationItem(i));
  }

  return {
    notifications,
    options: {
      stop_on_error: false,
      deduplicate: true,
    },
  };
}

// Main test function
export default function () {
  // Variable batch sizes for more realistic load
  const batchSize = randomIntBetween(Math.max(10, BATCH_SIZE - 20), BATCH_SIZE + 20);
  const payload = generateBatchRequest(batchSize);

  const res = http.post(
    `${BASE_URL}/api/v1/notifications/batch`,
    JSON.stringify(payload),
    { headers, tags: { name: 'batch_send' } }
  );

  batchRequests.add(1);
  notificationsInBatch.add(batchSize);
  batchLatency.add(res.timings.duration);

  const success = check(res, {
    'status is 200': (r) => r.status === 200,
    'response has batch_id': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.batch_id !== undefined;
      } catch {
        return false;
      }
    },
    'response has summary': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.summary !== undefined;
      } catch {
        return false;
      }
    },
  });

  batchSuccess.add(success ? 1 : 0);

  if (success) {
    try {
      const body = JSON.parse(res.body);
      if (body.summary) {
        notificationsSucceeded.add(body.summary.succeeded || 0);
        notificationsFailed.add(body.summary.failed || 0);
        deliveriesTotal.add(body.summary.total_delivered || 0);
      }
    } catch {
      // Ignore parse errors
    }
  } else {
    batchFailed.add(1);
    console.warn(`Batch failed: status=${res.status}`);
    if (res.status !== 200) {
      console.warn(`Response: ${res.body.substring(0, 200)}`);
    }
  }

  // Small sleep between batches
  sleep(Math.random() * 0.2);
}

// Setup function
export function setup() {
  console.log('='.repeat(60));
  console.log('Ara Notification Service - Batch API Load Test');
  console.log('='.repeat(60));
  console.log(`Target: ${BASE_URL}/api/v1/notifications/batch`);
  console.log(`Base Batch Size: ${BATCH_SIZE}`);
  console.log('='.repeat(60));

  // Health check
  const healthRes = http.get(`${BASE_URL}/health`);
  if (healthRes.status !== 200) {
    console.error('FATAL: Health check failed');
    return { error: true };
  }

  // Test batch endpoint with small batch
  const testPayload = generateBatchRequest(5);
  const testRes = http.post(
    `${BASE_URL}/api/v1/notifications/batch`,
    JSON.stringify(testPayload),
    { headers }
  );

  if (testRes.status !== 200) {
    console.error(`FATAL: Batch endpoint test failed: ${testRes.status}`);
    console.error(`Response: ${testRes.body}`);
    return { error: true };
  }

  console.log('Setup complete - batch endpoint verified');
  return { error: false };
}

// Teardown function
export function teardown(data) {
  console.log('='.repeat(60));
  console.log('Batch API Load Test Complete');
  console.log('='.repeat(60));

  if (data && data.error) {
    console.log('Test failed due to setup issues');
  }
}

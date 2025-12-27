/**
 * K6 Load Test Configuration
 *
 * Centralized configuration for all load test scenarios.
 * Import this file in your test scripts to use shared settings.
 *
 * Usage:
 *   import { config, profiles, thresholds } from './config.js';
 */

// Default configuration (can be overridden by environment variables)
export const config = {
  // Server endpoints
  host: __ENV.HOST || 'localhost:8081',
  get apiUrl() {
    return `http://${this.host}`;
  },
  get wsUrl() {
    return `ws://${this.host}/ws`;
  },

  // Authentication
  apiKey: __ENV.API_KEY || '',
  jwtToken: __ENV.JWT_TOKEN || '',

  // Test settings
  channels: (__ENV.CHANNELS || 'load-test,stress-test').split(','),
  connectionDuration: parseInt(__ENV.CONNECTION_DURATION || '60', 10) * 1000,
  batchSize: parseInt(__ENV.BATCH_SIZE || '50', 10),
};

// Load test profiles
export const profiles = {
  // Development/Quick tests
  smoke: {
    name: 'Smoke Test',
    description: 'Quick verification that everything works',
    wsConnections: 10,
    httpRate: 10,
    duration: '30s',
    rampUp: '10s',
    rampDown: '10s',
  },

  // Baseline performance
  baseline: {
    name: 'Baseline',
    description: '100 connections, 50 req/s - normal production load',
    wsConnections: 100,
    httpRate: 50,
    duration: '2m',
    rampUp: '30s',
    rampDown: '30s',
  },

  // Medium load
  medium: {
    name: 'Medium Load',
    description: '500 connections, 100 req/s - peak hour simulation',
    wsConnections: 500,
    httpRate: 100,
    duration: '3m',
    rampUp: '1m',
    rampDown: '30s',
  },

  // High load
  high: {
    name: 'High Load',
    description: '1000 connections, 200 req/s - high traffic',
    wsConnections: 1000,
    httpRate: 200,
    duration: '5m',
    rampUp: '1m',
    rampDown: '30s',
  },

  // Stress test
  stress: {
    name: 'Stress Test',
    description: '2000 connections, 500 req/s - find breaking point',
    wsConnections: 2000,
    httpRate: 500,
    duration: '5m',
    rampUp: '2m',
    rampDown: '1m',
  },

  // Soak test
  soak: {
    name: 'Soak Test',
    description: 'Extended duration test for memory leaks',
    wsConnections: 500,
    httpRate: 50,
    duration: '30m',
    rampUp: '2m',
    rampDown: '2m',
  },

  // Spike test
  spike: {
    name: 'Spike Test',
    description: 'Sudden traffic surge simulation',
    wsConnections: 100,
    httpRate: 1000, // Sudden spike
    duration: '1m',
    rampUp: '5s', // Very fast ramp
    rampDown: '30s',
  },
};

// Performance thresholds
export const thresholds = {
  // WebSocket thresholds
  websocket: {
    'connection_success_rate': ['rate>0.95'],
    'ws_connections_failed': ['count<50'],
    'message_latency_ms': ['p(95)<100', 'p(99)<500'],
  },

  // HTTP API thresholds
  http: {
    'request_success_rate': ['rate>0.99'],
    'request_latency_ms': ['p(95)<50', 'p(99)<100'],
    'http_req_duration': ['p(95)<100'],
  },

  // Batch API thresholds
  batch: {
    'batch_success_rate': ['rate>0.98'],
    'batch_latency_ms': ['p(95)<200', 'p(99)<500'],
  },

  // End-to-end thresholds
  e2e: {
    'overall_success_rate': ['rate>0.95'],
    'e2e_latency_ms': ['p(95)<200', 'p(99)<500'],
  },
};

// Test scenarios generators
export const scenarios = {
  /**
   * Generate WebSocket ramping stages
   */
  wsRampingStages(profile) {
    const p = profiles[profile] || profiles.baseline;
    return [
      { duration: p.rampUp, target: p.wsConnections / 2 },
      { duration: p.rampUp, target: p.wsConnections },
      { duration: p.duration, target: p.wsConnections },
      { duration: p.rampDown, target: 0 },
    ];
  },

  /**
   * Generate HTTP ramping stages
   */
  httpRampingStages(profile) {
    const p = profiles[profile] || profiles.baseline;
    return [
      { duration: p.rampUp, target: p.httpRate / 2 },
      { duration: p.rampUp, target: p.httpRate },
      { duration: p.duration, target: p.httpRate },
      { duration: p.rampDown, target: 0 },
    ];
  },
};

// Common headers
export const headers = {
  json: {
    'Content-Type': 'application/json',
    'X-API-Key': config.apiKey,
  },
};

// Helper functions
export const helpers = {
  /**
   * Get profile by name or return default
   */
  getProfile(name) {
    return profiles[name] || profiles.baseline;
  },

  /**
   * Log test configuration
   */
  logConfig(testName) {
    console.log('='.repeat(70));
    console.log(`Ara Notification Service - ${testName}`);
    console.log('='.repeat(70));
    console.log(`Host: ${config.host}`);
    console.log(`API URL: ${config.apiUrl}`);
    console.log(`WebSocket URL: ${config.wsUrl}`);
    console.log('='.repeat(70));
  },
};

export default {
  config,
  profiles,
  thresholds,
  scenarios,
  headers,
  helpers,
};

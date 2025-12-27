/**
 * JWT Token Generator for K6 Load Tests
 *
 * This module provides helper functions for generating JWT tokens
 * for use in load testing. In production, tokens should be generated
 * by your authentication service.
 *
 * Usage in K6 scripts:
 *   import { generateToken, generateTestTokens } from './utils/jwt-generator.js';
 *
 * Note: This is a simplified implementation for testing purposes.
 * Real JWT generation requires proper cryptographic libraries.
 */

import encoding from 'k6/encoding';
import { randomString } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

/**
 * Base64 URL encode (JWT-compatible)
 */
function base64UrlEncode(str) {
  const base64 = encoding.b64encode(str, 'rawstd');
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

/**
 * Create an unsigned JWT payload (for testing only)
 * Note: This creates a valid JWT structure but with a dummy signature.
 * The server must be configured to accept these tokens for load testing.
 *
 * @param {Object} claims - JWT claims
 * @returns {string} JWT token string
 */
export function createTestToken(claims) {
  const header = {
    alg: 'HS256',
    typ: 'JWT',
  };

  const now = Math.floor(Date.now() / 1000);
  const payload = {
    sub: claims.sub || `test-user-${randomString(8)}`,
    exp: claims.exp || now + 3600, // 1 hour from now
    iat: now,
    roles: claims.roles || ['user'],
    ...claims,
  };

  const headerB64 = base64UrlEncode(JSON.stringify(header));
  const payloadB64 = base64UrlEncode(JSON.stringify(payload));

  // For testing, we use a placeholder signature
  // In production, the token should be properly signed
  const signature = base64UrlEncode('test-signature-placeholder');

  return `${headerB64}.${payloadB64}.${signature}`;
}

/**
 * Generate multiple test tokens
 *
 * @param {number} count - Number of tokens to generate
 * @param {Object} options - Token options
 * @returns {Array<string>} Array of JWT tokens
 */
export function generateTestTokens(count, options = {}) {
  const tokens = [];

  for (let i = 0; i < count; i++) {
    const claims = {
      sub: options.userPrefix ? `${options.userPrefix}-${i}` : `load-test-user-${i}`,
      roles: options.roles || ['user'],
      ...options.claims,
    };

    tokens.push(createTestToken(claims));
  }

  return tokens;
}

/**
 * Generate a single token with random user ID
 *
 * @param {Object} options - Token options
 * @returns {string} JWT token
 */
export function generateRandomToken(options = {}) {
  return createTestToken({
    sub: `load-test-${randomString(12)}`,
    roles: options.roles || ['user'],
    ...options,
  });
}

/**
 * Parse a JWT token (for debugging)
 *
 * @param {string} token - JWT token string
 * @returns {Object} Parsed token with header and payload
 */
export function parseToken(token) {
  const parts = token.split('.');
  if (parts.length !== 3) {
    throw new Error('Invalid JWT format');
  }

  const header = JSON.parse(encoding.b64decode(parts[0], 'rawstd', 's'));
  const payload = JSON.parse(encoding.b64decode(parts[1], 'rawstd', 's'));

  return { header, payload };
}

// Export for CommonJS compatibility
export default {
  createTestToken,
  generateTestTokens,
  generateRandomToken,
  parseToken,
};

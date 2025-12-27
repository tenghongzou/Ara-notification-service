#!/bin/bash
#
# Load Test Runner Script
# Usage: ./run-tests.sh [test] [profile]
#
# Examples:
#   ./run-tests.sh                    # Run all tests with baseline profile
#   ./run-tests.sh websocket          # Run websocket test
#   ./run-tests.sh http-api high      # Run HTTP API test with high profile
#   ./run-tests.sh e2e stress         # Run e2e test with stress profile
#

set -e

# Default values
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_TYPE="${1:-all}"
PROFILE="${2:-baseline}"
HOST="${HOST:-localhost:8081}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if k6 is installed
if ! command -v k6 &> /dev/null; then
    echo -e "${RED}Error: k6 is not installed${NC}"
    echo "Install k6: https://k6.io/docs/getting-started/installation/"
    exit 1
fi

# Check for required environment variables
check_env() {
    if [ -z "$JWT_TOKEN" ]; then
        echo -e "${YELLOW}Warning: JWT_TOKEN not set, WebSocket tests may fail${NC}"
    fi
    if [ -z "$API_KEY" ]; then
        echo -e "${YELLOW}Warning: API_KEY not set, HTTP tests may fail${NC}"
    fi
}

# Run a test
run_test() {
    local test_file="$1"
    local test_name="$2"

    echo -e "${GREEN}Running: ${test_name}${NC}"
    echo "Profile: ${PROFILE}"
    echo "Host: ${HOST}"
    echo "---"

    k6 run \
        --env HOST="${HOST}" \
        --env WS_HOST="${HOST}" \
        --env API_HOST="${HOST}" \
        --env JWT_TOKEN="${JWT_TOKEN:-}" \
        --env API_KEY="${API_KEY:-}" \
        --env PROFILE="${PROFILE}" \
        "${SCRIPT_DIR}/${test_file}"

    echo ""
}

# Health check
health_check() {
    echo "Checking server health..."
    if curl -sf "http://${HOST}/health" > /dev/null 2>&1; then
        echo -e "${GREEN}Server is healthy${NC}"
    else
        echo -e "${RED}Server health check failed${NC}"
        echo "Please ensure the server is running at ${HOST}"
        exit 1
    fi
}

# Main
echo "================================"
echo "Ara Notification Service"
echo "Load Testing Suite"
echo "================================"
echo ""

check_env
health_check
echo ""

case "${TEST_TYPE}" in
    websocket|ws)
        run_test "websocket.js" "WebSocket Load Test"
        ;;
    http|http-api)
        run_test "http-api.js" "HTTP API Load Test"
        ;;
    batch|batch-api)
        run_test "batch-api.js" "Batch API Load Test"
        ;;
    e2e|e2e-load)
        run_test "e2e-load.js" "End-to-End Load Test"
        ;;
    all)
        run_test "websocket.js" "WebSocket Load Test"
        run_test "http-api.js" "HTTP API Load Test"
        run_test "batch-api.js" "Batch API Load Test"
        run_test "e2e-load.js" "End-to-End Load Test"
        ;;
    *)
        echo "Usage: $0 [test] [profile]"
        echo ""
        echo "Tests:"
        echo "  websocket  - WebSocket connection test"
        echo "  http-api   - HTTP API test"
        echo "  batch-api  - Batch API test"
        echo "  e2e        - End-to-end test"
        echo "  all        - Run all tests"
        echo ""
        echo "Profiles: smoke, baseline, medium, high, stress, soak, spike"
        exit 1
        ;;
esac

echo -e "${GREEN}Load testing completed${NC}"

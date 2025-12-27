@echo off
REM Load Test Runner Script for Windows
REM Usage: run-tests.bat [test] [profile]
REM
REM Examples:
REM   run-tests.bat                    - Run all tests with baseline profile
REM   run-tests.bat websocket          - Run websocket test
REM   run-tests.bat http-api high      - Run HTTP API test with high profile
REM   run-tests.bat e2e stress         - Run e2e test with stress profile

setlocal enabledelayedexpansion

REM Default values
set "TEST_TYPE=%~1"
set "PROFILE=%~2"

if "%TEST_TYPE%"=="" set "TEST_TYPE=all"
if "%PROFILE%"=="" set "PROFILE=baseline"
if "%HOST%"=="" set "HOST=localhost:8081"

REM Get script directory
set "SCRIPT_DIR=%~dp0"

echo ================================
echo Ara Notification Service
echo Load Testing Suite
echo ================================
echo.

REM Check if k6 is installed
where k6 >nul 2>nul
if %errorlevel% neq 0 (
    echo Error: k6 is not installed
    echo Install k6: https://k6.io/docs/getting-started/installation/
    exit /b 1
)

REM Check for required environment variables
if "%JWT_TOKEN%"=="" (
    echo Warning: JWT_TOKEN not set, WebSocket tests may fail
)
if "%API_KEY%"=="" (
    echo Warning: API_KEY not set, HTTP tests may fail
)

REM Health check
echo Checking server health...
curl -sf "http://%HOST%/health" >nul 2>nul
if %errorlevel% neq 0 (
    echo Server health check failed
    echo Please ensure the server is running at %HOST%
    exit /b 1
)
echo Server is healthy
echo.

REM Run tests based on type
if "%TEST_TYPE%"=="websocket" goto :websocket
if "%TEST_TYPE%"=="ws" goto :websocket
if "%TEST_TYPE%"=="http" goto :http
if "%TEST_TYPE%"=="http-api" goto :http
if "%TEST_TYPE%"=="batch" goto :batch
if "%TEST_TYPE%"=="batch-api" goto :batch
if "%TEST_TYPE%"=="e2e" goto :e2e
if "%TEST_TYPE%"=="e2e-load" goto :e2e
if "%TEST_TYPE%"=="all" goto :all
goto :usage

:websocket
echo Running: WebSocket Load Test
echo Profile: %PROFILE%
echo Host: %HOST%
echo ---
k6 run --env HOST=%HOST% --env WS_HOST=%HOST% --env JWT_TOKEN=%JWT_TOKEN% --env PROFILE=%PROFILE% "%SCRIPT_DIR%websocket.js"
goto :end

:http
echo Running: HTTP API Load Test
echo Profile: %PROFILE%
echo Host: %HOST%
echo ---
k6 run --env HOST=%HOST% --env API_HOST=%HOST% --env API_KEY=%API_KEY% --env PROFILE=%PROFILE% "%SCRIPT_DIR%http-api.js"
goto :end

:batch
echo Running: Batch API Load Test
echo Profile: %PROFILE%
echo Host: %HOST%
echo ---
k6 run --env HOST=%HOST% --env API_HOST=%HOST% --env API_KEY=%API_KEY% --env PROFILE=%PROFILE% "%SCRIPT_DIR%batch-api.js"
goto :end

:e2e
echo Running: End-to-End Load Test
echo Profile: %PROFILE%
echo Host: %HOST%
echo ---
k6 run --env HOST=%HOST% --env JWT_TOKEN=%JWT_TOKEN% --env API_KEY=%API_KEY% --env PROFILE=%PROFILE% "%SCRIPT_DIR%e2e-load.js"
goto :end

:all
call :websocket
echo.
call :http
echo.
call :batch
echo.
call :e2e
goto :end

:usage
echo Usage: %~nx0 [test] [profile]
echo.
echo Tests:
echo   websocket  - WebSocket connection test
echo   http-api   - HTTP API test
echo   batch-api  - Batch API test
echo   e2e        - End-to-end test
echo   all        - Run all tests
echo.
echo Profiles: smoke, baseline, medium, high, stress, soak, spike
exit /b 1

:end
echo.
echo Load testing completed
endlocal

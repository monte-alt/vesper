@echo off
setlocal EnableExtensions
title Vesper - Windows x64 DX12 Runtime Launcher

rem Run as your normal Windows user (not Administrator) so GPU shader caches stay consistent.

rem Clear inherited VESPER_* settings except path configuration variables
for /f "delims==" %%A in ('set VESPER_ 2^>nul') do (
    if /i not "%%A"=="VESPER_CONFIG" (
    if /i not "%%A"=="VESPER_GAME_DIR" (
    if /i not "%%A"=="VESPER_CACHE_DIR" (
    if /i not "%%A"=="VESPER_DATA_DIR" (
    if /i not "%%A"=="VESPER_FIDELITYFX_DIR" (
        set "%%A="
    )))))
)

set "ROOT_DIR=%~dp0.."
set "VESPER_EXE=%ROOT_DIR%\target\release\vesper.exe"
set "AGILITY_DLL=%ROOT_DIR%\target\release\D3D12\D3D12Core.dll"
set "TITLE_EXE=%~1"
if "%TITLE_EXE%"=="" set "TITLE_EXE=D:\path\to\your\title.exe"

if not exist "%VESPER_EXE%" (
    echo ERROR: Missing %VESPER_EXE%
    echo Build it first with: cargo +1.98.1-x86_64-pc-windows-msvc build --release --locked --bin vesper
    pause
    exit /b 1
)
if not exist "%TITLE_EXE%" (
    echo ERROR: Missing title executable: %TITLE_EXE%
    echo Usage: scripts\launch.cmd "D:\path\to\your\title.exe"
    pause
    exit /b 1
)
if not exist "%AGILITY_DLL%" (
    echo NOTE: Agility SDK D3D12Core.dll not found at %AGILITY_DLL%; falling back to inbox D3D12.
)

echo.
echo ============================================================================
echo   Vesper Runtime Launcher (RX 9070 XT / 32 GB RAM Tuned Presets)
echo   MSVC Native Build + Agility SDK 1.614.1 + DXGI Allow-Tearing
echo ============================================================================
echo   [1] Native 1080p Series X - NO FSR (Borderless) : AFMF 2 Ready Default [Default]
echo   [2] Native 1440p Series X - NO FSR (Borderless) : Maximum Native Crispness
echo   [3] Series S Native - NO FSR (Borderless)       : Highest Base FPS / Lowest RAM
echo   [4] 1080p Series X + FSR 4.1.2 Quality          : Optional FidelityFX Upscale
echo   [5] 1440p Series X + RDNA 4 HW RT               : Experimental HW BVH4 DXR
echo.
echo   TIP: Enable AMD AFMF 2 in Adrenalin (or Lossless Scaling 2x) on [1] or [3].
echo   HOTKEYS: ~ Vesper Menu  ^|  F3 Cycle HUD (Basic / Advanced / Off)  ^|  F2 Toggle Mouse Look
echo.
choice /C 12345 /N /T 5 /D 1 /M "Select launch mode [1-5] (auto-starts [1] in 5s): "
set "MODE=%ERRORLEVEL%"

rem --- Core scheduling, command-list threading, enhanced barriers & RX 9070 XT tuning ---
set "VESPER_TEXTURE_NO_WAIT=1"
set "VESPER_RELAY=2"
set "VESPER_PARALLEL_READBACK=1"
set "VESPER_CARD_REHASH_RUNS=1"
set "VESPER_JOIN_COPIES=1"
set "VESPER_INDIRECT_WINDOW=1"
set "VESPER_RELAY_COPY=1"
set "VESPER_CARD_STEPS=2"
set "VESPER_INDIRECT_BATCH=2"
set "VESPER_MAP_THREADS=1"
set "VESPER_TITLE_PRIORITIES=0"
set "VESPER_GPU_KEEP_AWAKE=1"
set "VESPER_ASYNC_PRESENT=1"
set "VESPER_PARALLEL_LISTS=1"
set "VESPER_UPLOAD_AHEAD=1"
set "VESPER_D3D12_LOW_PRIORITY=0"
set "VESPER_SUBMIT_FRAMES=3"
set "VESPER_UPLOAD_THREADS=6"
set "VESPER_UPLOAD_SKIP_CLEAN=1"
set "VESPER_COALESCE_BARRIERS=1"
set "VESPER_MERGE_LISTS=96"
set "VESPER_COPY_QUEUE=0"

rem --- Memory, ReBAR, In-Place Hot Regions & Anti-Stutter Frame Pacing (16 GB VRAM / 32 GB RAM) ---
set "VESPER_RESIDENCY=1"
set "VESPER_PRESSURE_PHYSICAL=1"
set "VESPER_PRESSURE_FREE_MB=2560"
set "VESPER_WAIT_RESIDENT=textures"
set "VESPER_TEXTURE_STAGE_MS=14"
set "VESPER_TEXTURE_STAGE_MB=48"
set "VESPER_TEXTURE_CATCHUP=0"
set "VESPER_KEEP_FREED=0"
set "VESPER_WINDOW_BUDGET_MB=512"
set "VESPER_REBAR_WINDOW_MB=1024"
set "VESPER_REBAR_STAGING=1"
set "VESPER_REBAR_UPLOAD=1"
set "VESPER_WINDOW_KB=512"
set "VESPER_WINDOW_TAILS=1"
set "VESPER_DEFER_FREE_FRAMES=4"
set "VESPER_TEXTURE_MAX=2048"
set "VESPER_CARD_BUFFERS_MIN_KB=512"
set "VESPER_INPLACE_HOT=1"
set "VESPER_INPLACE_HOT_MB=768"

rem --- Renderer ports + AVX2 streaming stores + 8-core threading ---
set "VESPER_PORT_CONSTANTS=1"
set "VESPER_PORT_DESCRIPTORS=1"
set "VESPER_PORT_CBV=1"
set "VESPER_PORT_JOURNAL=1"
set "VESPER_PORT_GUARD=1"
set "VESPER_PORT_RT=1"
set "VESPER_STREAM_COPY=1"
set "VESPER_STREAM_REFRESH=1"
set "VESPER_JOIN_PARTS=16"
set "VESPER_LIST_THREADS=8"
set "VESPER_LANE_CORE=1"
set "VESPER_WORKER_SPIN=100"
set "VESPER_ASK_SPREAD=1"
set "VESPER_PIPELINE_WORKERS=2"

rem --- Calibrated interior lighting & reflections ---
set "VESPER_AMBIENT=0.8"
set "VESPER_AMBIENT_AIR=0.25"
set "VESPER_AMBIENT_SHADE=1.35,1.1"
set "VESPER_AMBIENT_REFLECT=1.0"
set "VESPER_AMBIENT_SETTLE=250,3,2.0"
set "VESPER_AMBIENT_FLOOR=0.01"

rem Vesper HUD + interactive ~ Control Center Menu + PC keyboard & mouse mapping
set "VESPER_PANEL=1"
set "VESPER_KBM_PC=1"

if "%MODE%"=="1" (
    set "CONSOLE=series-x"
    set "VESPER_OUTPUT=1920x1080"
    set "VESPER_BORDERLESS=1"
) else if "%MODE%"=="2" (
    set "CONSOLE=series-x"
    set "VESPER_OUTPUT=2560x1440"
    set "VESPER_BORDERLESS=1"
) else if "%MODE%"=="3" (
    set "CONSOLE=series-s"
    set "VESPER_OUTPUT="
    set "VESPER_FSR_4K="
    set "VESPER_BORDERLESS=1"
) else if "%MODE%"=="4" (
    set "CONSOLE=series-x"
    set "VESPER_OUTPUT=1920x1080"
    set "VESPER_FSR_4K=quality"
    set "VESPER_BORDERLESS=1"
) else if "%MODE%"=="5" (
    set "CONSOLE=series-x"
    set "VESPER_OUTPUT=2560x1440"
    set "VESPER_BORDERLESS=1"
    set "VESPER_BVH_BUILDER=1"
    set "VESPER_HW_BVH=1"
    set "VESPER_BVH_FENCES=0"
    set "VESPER_AMBIENT_RT=0.7,1.5"
)

echo.
echo Launching mode %MODE% (--console %CONSOLE%)...
pushd "%ROOT_DIR%"
"%VESPER_EXE%" run "%TITLE_EXE%" --execute --console %CONSOLE%
set "EXIT_CODE=%ERRORLEVEL%"
popd

if not "%EXIT_CODE%"=="0" (
    echo.
    echo Runtime exited with code %EXIT_CODE%.
    pause
)

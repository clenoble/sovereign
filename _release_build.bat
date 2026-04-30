@echo off
cd /d "%~dp0"
set "CUDA_PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.2"
set "PATH=%CUDA_PATH%\bin\x64;%CUDA_PATH%\bin;%PATH%"
set "SOVEREIGN_TARGET_DIR=C:\cargo-target"
call _build.bat build --release -p sovereign-app --features cuda,encryption,p2p,comms-email,comms-signal,web-browse -j 4
echo EXIT=%ERRORLEVEL%

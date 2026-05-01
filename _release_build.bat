@echo off
cd /d "%~dp0"
set "CUDA_PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.2"
REM MSBuild's CUDA targets resolve CudaToolkitDir from CUDA_PATH_V13_2 (or
REM CudaToolkitDir directly). Set both so the build works in shells that
REM predate the CUDA install and didn't inherit these from the system env.
set "CUDA_PATH_V13_2=%CUDA_PATH%"
set "CudaToolkitDir=%CUDA_PATH%\"
set "PATH=%CUDA_PATH%\bin\x64;%CUDA_PATH%\bin;%PATH%"
set "SOVEREIGN_TARGET_DIR=C:\cargo-target"
call "%~dp0_build.bat" build --release -p sovereign-app --features cuda,encryption,p2p,comms-email,web-browse -j 4
echo EXIT=%ERRORLEVEL%

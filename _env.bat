@echo off
REM Shared Windows build environment for the release and gate scripts
REM (_release_build.bat, _release_tauri_frozen.bat, _prepublish.bat).
REM CALL it from a script that has run `setlocal`; it must not run setlocal
REM itself, or its variables would vanish when it returns.
REM
REM Same variables and defaults as _build.bat / _check.bat / _test.bat:
REM   SOVEREIGN_LLVM_DIR      LLVM bin dir with libclang.dll  (C:\Program Files\LLVM\bin)
REM   SOVEREIGN_CMAKE_DIR     CMake bin dir                    (C:\Program Files\CMake\bin)
REM   SOVEREIGN_TARGET_DIR    cargo target dir                 (C:\cargo-target, as _test.bat:
REM                           Smart App Control can block binaries built under the
REM                           workspace target\ dir, a local-drive target avoids it)
REM plus:
REM   SOVEREIGN_CLANG_INCLUDE clang's resource include dir, passed to bindgen
REM                           (rocksdb) through BINDGEN_EXTRA_CLANG_ARGS. Without it
REM                           some machines fail with "'stddef.h' file not found".
REM                           Default: auto-detected under <LLVM>\lib\clang\<ver>\include,
REM                           as an 8.3 short path (a space would split the clang arg).
REM                           Set it to "none" to pass nothing. Ignored when
REM                           BINDGEN_EXTRA_CLANG_ARGS is already set.
REM   SOVEREIGN_JOBS          cargo -j value (4)

if not defined SOVEREIGN_LLVM_DIR   set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR  set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"
if not defined SOVEREIGN_TARGET_DIR set "SOVEREIGN_TARGET_DIR=C:\cargo-target"
if not defined SOVEREIGN_JOBS       set "SOVEREIGN_JOBS=4"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"
set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"

if defined BINDGEN_EXTRA_CLANG_ARGS goto :clang_done
if /i "%SOVEREIGN_CLANG_INCLUDE%"=="none" goto :clang_done
if defined SOVEREIGN_CLANG_INCLUDE goto :clang_set
REM Auto-detect: the last version directory found wins (usually there is one).
for /d %%v in ("%SOVEREIGN_LLVM_DIR%\..\lib\clang\*") do (
    if exist "%%~v\include\stddef.h" set "SOVEREIGN_CLANG_INCLUDE=%%~sv\include"
)
if not defined SOVEREIGN_CLANG_INCLUDE goto :clang_done
:clang_set
set "BINDGEN_EXTRA_CLANG_ARGS=-I%SOVEREIGN_CLANG_INCLUDE%"
:clang_done

echo [env] target=%CARGO_TARGET_DIR%  jobs=%SOVEREIGN_JOBS%  bindgen-args=%BINDGEN_EXTRA_CLANG_ARGS%
exit /b 0

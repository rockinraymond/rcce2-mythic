@echo off
setlocal

set "ROOTDIR=%~dp0"
if "%ROOTDIR:~-1%"=="\" set "ROOTDIR=%ROOTDIR:~0,-1%"

set "BLITZPATH=%ROOTDIR%\compiler\BlitzForge"
set "TESTDIR="

if exist "%ROOTDIR%\src\Tests" (
    set "TESTDIR=%ROOTDIR%\src\Tests"
) else if exist "%ROOTDIR%\src\tests" (
    set "TESTDIR=%ROOTDIR%\src\tests"
)

if not exist "%BLITZPATH%\bin\blitzcc.exe" (
    echo "%BLITZPATH%\bin\blitzcc.exe not found!"
    echo "Compile source or download binaries from https://github.com/RydeTec/blitz-forge/releases"
    exit /b 1
)

if not defined TESTDIR (
    echo "Test directory not found. Expected src\Tests or src\tests."
    exit /b 1
)

cd /d "%TESTDIR%"

set FAILED=0

for /R %%f in (*.bb) do (
    "%BLITZPATH%\bin\blitzcc.exe" -t -w "%ROOTDIR%\src" "%%f" || (echo "%%f failed at least one test" && SET FAILED=1)
)

cd /d "%ROOTDIR%"

if %FAILED% == 1 (
    echo "Tests failed"
    endlocal
    exit /b 1
)

echo "Tests passed"

endlocal

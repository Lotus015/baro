@echo off
set "SCRIPT_DIR=%~dp0"
set "NATIVE=%SCRIPT_DIR%baro-native.exe"
if exist "%NATIVE%" (
    "%NATIVE%" %*
) else (
    echo baro: binary not installed. Run: npm rebuild baro-ai 1>&2
    exit /b 1
)

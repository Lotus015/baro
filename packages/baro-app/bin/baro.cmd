@echo off
set "NATIVE=%USERPROFILE%\.baro\bin\baro.exe"
if exist "%NATIVE%" (
    "%NATIVE%" %*
) else (
    echo baro: binary not installed. Run: npm rebuild baro-ai 1>&2
    exit /b 1
)

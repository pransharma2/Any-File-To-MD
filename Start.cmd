@echo off
rem Double-click me. First run installs everything automatically (about a minute).
title Any File To MD
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Start.ps1" %*
if errorlevel 1 (
    echo.
    echo Something went wrong. See the message above.
    pause
)

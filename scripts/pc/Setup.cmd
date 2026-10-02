@echo off
rem Double-click for one-time PC setup (tools, login, scheduled resume).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-pc.ps1"
pause

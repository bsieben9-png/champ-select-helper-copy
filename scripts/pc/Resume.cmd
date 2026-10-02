@echo off
rem Double-click to resume the project with Claude Code (opens a window).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0resume.ps1" -Mode Interactive

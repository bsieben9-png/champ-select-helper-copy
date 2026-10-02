@echo off
rem Double-click to put the newest app build in %USERPROFILE%\ChampSelectHelper\latest
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0get-build.ps1"
pause

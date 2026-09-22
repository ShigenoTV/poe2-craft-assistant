@echo off
setlocal
pushd "%~dp0"
title PoE2 Craft Assistant - controle de la derniere Release
where node >nul 2>nul
if errorlevel 1 (
    echo Node.js est introuvable. Lance d abord build-installer.bat.
    pause
    exit /b 1
)
node tools\check-release.mjs %*
pause

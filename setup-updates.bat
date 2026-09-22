@echo off
setlocal
pushd "%~dp0"
title PoE2 Craft Assistant - configuration des mises a jour
where node >nul 2>nul
if errorlevel 1 (
    echo Node.js est introuvable. Lance d abord build-installer.bat.
    pause
    exit /b 1
)
if not exist node_modules (
    call npm ci
    if errorlevel 1 goto :fail
)
node tools\setup-updates.mjs %*
if errorlevel 1 goto :fail
pause
exit /b 0

:fail
echo.
echo ECHEC : lis les messages ci-dessus.
pause
exit /b 1

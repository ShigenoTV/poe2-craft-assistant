@echo off
setlocal
pushd "%~dp0"
title PoE2 Craft Assistant - publier une version
where node >nul 2>nul
if errorlevel 1 (
    echo Node.js est introuvable. Lance d abord build-installer.bat.
    pause
    exit /b 1
)
set /p VERSION=Numero de la nouvelle version, par exemple 0.2.0 : 
node tools\release.mjs %VERSION%
if errorlevel 1 goto :fail
pause
exit /b 0

:fail
echo.
echo ECHEC : lis les messages ci-dessus.
pause
exit /b 1

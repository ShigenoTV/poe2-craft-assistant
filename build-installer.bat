@echo off
setlocal EnableExtensions
pushd "%~dp0"
title PoE2 Craft Assistant - construction de l'installeur

echo.
echo  Construction de l'installeur Windows (premier lancement : 10 a 20 minutes)
echo.

where winget >nul 2>nul
if errorlevel 1 (
    echo winget est introuvable. Il est fourni avec Windows 10 1809 ou plus recent et Windows 11.
    echo Installe "Programme d'installation d'application" depuis le Microsoft Store, puis relance ce fichier.
    goto :fail
)

set NEEDS_RESTART=0

where node >nul 2>nul
if errorlevel 1 (
    echo [1/4] Installation de Node.js...
    winget install --id OpenJS.NodeJS.LTS -e --accept-source-agreements --accept-package-agreements
    if errorlevel 1 goto :fail
    set NEEDS_RESTART=1
)

where cargo >nul 2>nul
if errorlevel 1 (
    echo [2/4] Installation de Rust...
    winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
    if errorlevel 1 goto :fail
    set NEEDS_RESTART=1
)

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set HAS_CPP=0
if exist "%VSWHERE%" (
    for /f "usebackq delims=" %%i in (`"%VSWHERE%" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set HAS_CPP=1
)
if "%HAS_CPP%"=="0" (
    echo [3/4] Installation des outils de build C++ - long, ne ferme pas la fenetre...
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-source-agreements --accept-package-agreements --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
    if errorlevel 1 goto :fail
    set NEEDS_RESTART=1
)

if "%NEEDS_RESTART%"=="1" (
    echo.
    echo Des outils viennent d'etre installes.
    echo Ferme cette fenetre, puis double-clique de nouveau sur build-installer.bat.
    pause
    exit /b 0
)

echo [4/4] Compilation...
rustup default stable
if errorlevel 1 goto :fail
call npm ci
if errorlevel 1 goto :fail
call npm run tauri build -- --bundles nsis
if errorlevel 1 goto :fail

echo.
echo Installeur cree dans : %CD%\target\release\bundle\nsis
explorer "%CD%\target\release\bundle\nsis"
pause
exit /b 0

:fail
echo.
echo ECHEC : lis les messages ci-dessus.
echo Si le probleme persiste, copie les dernieres lignes rouges et envoie-les moi.
pause
exit /b 1

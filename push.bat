@echo off
setlocal EnableDelayedExpansion
pushd "%~dp0"
title PoE2 Craft Assistant - envoyer sur GitHub

where git >nul 2>nul
if errorlevel 1 (
    echo Git est introuvable. Installe-le depuis https://git-scm.com/download/win puis relance.
    goto :fail
)

git rev-parse --is-inside-work-tree >nul 2>nul
if errorlevel 1 (
    echo Ce dossier n'est pas un depot Git. Lance d'abord :
    echo   git init -b main
    echo   git remote add origin https://github.com/TOI/DEPOT.git
    goto :fail
)

set /p REFRESH=Rafraichir les donnees du jeu avant d'envoyer (update-dataset.bat) ? (o/N) :
if /i "%REFRESH%"=="o" (
    call update-dataset.bat
    if errorlevel 1 goto :fail
    echo.
)

echo.
echo Fichiers modifies :
git status --short
echo.

git diff --cached --quiet
set STAGED=%errorlevel%
git diff --quiet
set UNSTAGED=%errorlevel%
if %STAGED%==0 if %UNSTAGED%==0 (
    for /f %%i in ('git status --porcelain ^| find /c /v ""') do set N=%%i
    if "!N!"=="0" (
        echo Rien a envoyer, tout est deja a jour.
        pause
        exit /b 0
    )
)

set /p MSG=Message du commit (vide = "Mise a jour") :
if "%MSG%"=="" set MSG=Mise a jour

git add -A
if errorlevel 1 goto :fail

git commit -m "%MSG%"
if errorlevel 1 goto :fail

git push
if errorlevel 1 goto :fail

echo.
echo Envoye sur GitHub.
pause
exit /b 0

:fail
echo.
echo ECHEC : lis les messages ci-dessus.
pause
exit /b 1

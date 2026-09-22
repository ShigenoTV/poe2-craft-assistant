@echo off
setlocal EnableExtensions EnableDelayedExpansion
pushd "%~dp0"
title PoE2 Craft Assistant - mise a jour des donnees du jeu

echo.
echo  Telechargement des dernieres donnees depuis repoe-fork.github.io/poe2...
echo.

where node >nul 2>nul
if errorlevel 1 (
    echo Node.js est introuvable. Lance d'abord build-installer.bat, ou installe-le depuis nodejs.org.
    goto :fail
)
where curl >nul 2>nul
if errorlevel 1 (
    echo curl est introuvable ^(fourni avec Windows 10/11 normalement^). Mets Windows a jour, ou telecharge
    echo les deux fichiers a la main depuis https://repoe-fork.github.io/poe2/ et relance ce script.
    goto :fail
)

set "TMPDIR=%TEMP%\poe2-craft-dataset-update"
if not exist "%TMPDIR%" mkdir "%TMPDIR%"

echo [1/3] mods.min.json...
curl -L -f -o "%TMPDIR%\mods.min.json" "https://repoe-fork.github.io/poe2/mods.min.json"
if errorlevel 1 (
    echo Echec du telechargement. Verifie ta connexion, ou que le site est bien accessible dans un navigateur.
    goto :fail
)

echo [2/3] base_items.min.json...
curl -L -f -o "%TMPDIR%\base_items.min.json" "https://repoe-fork.github.io/poe2/base_items.min.json"
if errorlevel 1 (
    echo Echec du telechargement. Verifie ta connexion, ou que le site est bien accessible dans un navigateur.
    goto :fail
)

echo [3/3] Conversion...
node tools\import_repoe.mjs "%TMPDIR%\mods.min.json" "%TMPDIR%\base_items.min.json" -o data\sample\dataset.json
if errorlevel 1 goto :fail

echo.
echo Donnees mises a jour dans data\sample\dataset.json ^(c'est le jeu de donnees embarque dans l'application^).
echo.
echo Prochaines etapes :
echo   1. Verifie que l'application compile encore et que tout marche ^(npm run tauri dev^).
echo   2. Si c'est bon :  git add -A ^&^& git commit -m "Mise a jour des donnees de jeu" ^&^& git push
echo   3. Publie une nouvelle version avec release.bat pour que les utilisateurs la recoivent.
pause
exit /b 0

:fail
echo.
echo ECHEC : lis les messages ci-dessus.
pause
exit /b 1

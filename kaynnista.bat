@echo off
if exist "%~dp0releases\latest\Viima.exe" (
    start "" "%~dp0releases\latest\Viima.exe"
    exit /b 0
)
if exist "%~dp0releases\latest\OmaSelain.exe" (
    start "" "%~dp0releases\latest\OmaSelain.exe"
    exit /b 0
)
if exist "%~dp0target\release\viima.exe" (
    start "" "%~dp0target\release\viima.exe"
    exit /b 0
)
if exist "%~dp0target\release\oma_selain.exe" (
    start "" "%~dp0target\release\oma_selain.exe"
    exit /b 0
)
echo Viima-selainta ei loydy. Aja ensin scripts\build_release.ps1.
pause
exit /b 1

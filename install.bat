@echo off
:: D2R Suite — Quick Installer
:: Double-click this file or run from command prompt
:: Requires PowerShell and Rust (cargo)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
pause

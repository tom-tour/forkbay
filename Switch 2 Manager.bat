@echo off
start "" powershell -NoProfile -ExecutionPolicy Bypass -STA -WindowStyle Hidden -File "%~dp0Files\Manager\Manager.ps1"

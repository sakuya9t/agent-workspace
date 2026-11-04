@echo off
rem Windows entry point for the service scripts, from PowerShell or cmd:
rem
rem     scripts\asm start [--no-ui ...]     scripts\asm status
rem     scripts\asm restart-daemon          scripts\asm stop [daemon|asmux|ui|relay]
rem     scripts\asm token                   scripts\asm wizard
rem
rem It runs scripts\lib\asm.ps1 (which hands off to Git Bash) with a per-process
rem execution-policy bypass, so it works even where PowerShell's policy forbids
rem running scripts -- the Windows client default.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0lib\asm.ps1" %*
exit /b %ERRORLEVEL%

<#
.SYNOPSIS
  Run an Agent Session Manager service script on Windows. Use it through
  scripts\asm.cmd.

.DESCRIPTION
  The service scripts (start.sh, stop.sh, status.sh, ...) are bash, and on
  Windows they run under Git Bash. This forwards to them so PowerShell and cmd
  users need not open a Git Bash window. It finds Git Bash explicitly rather
  than running a bare `bash`, which on Windows can be System32's WSL launcher --
  that would run the scripts inside Linux, against a different filesystem.

  scripts\asm.cmd runs this with a per-process execution-policy bypass, so it
  works where PowerShell's policy forbids scripts (the Windows client default).
  Every argument after the command is passed through unchanged; see
  `scripts\asm start --help`.

.EXAMPLE
  scripts\asm start
  scripts\asm start --no-ui
  scripts\asm status
  scripts\asm restart-daemon
  scripts\asm stop
#>
# No param() block on purpose: flags like --no-ui must reach the script as
# plain strings, not be parsed as PowerShell parameters.

$commands = [ordered]@{
  'start'          = 'start.sh'
  'stop'           = 'stop.sh'
  'status'         = 'status.sh'
  'restart-daemon' = 'restart-daemon.sh'
  'token'          = 'token.sh'
  'wizard'         = 'wizard.sh'
  'setup'          = 'setup.sh'
}

if ($args.Count -eq 0 -or -not $commands.Contains([string]$args[0])) {
  Write-Host "usage: scripts\asm <$($commands.Keys -join '|')> [options]"
  exit 2
}

$bash = $null
foreach ($root in @($env:ProgramFiles, ${env:ProgramFiles(x86)}, (Join-Path $env:LOCALAPPDATA 'Programs'))) {
  if (-not $root) { continue }
  $candidate = Join-Path $root 'Git\bin\bash.exe'
  if (Test-Path $candidate) { $bash = $candidate; break }
}
if (-not $bash) {
  $git = Get-Command git -ErrorAction SilentlyContinue
  if ($git) {
    $candidate = Join-Path (Split-Path (Split-Path $git.Source)) 'bin\bash.exe'
    if (Test-Path $candidate) { $bash = $candidate }
  }
}
if (-not $bash) {
  Write-Host 'Git Bash not found. Install Git for Windows (winget install Git.Git), or run scripts\setup.ps1.' -ForegroundColor Red
  exit 1
}

# The service scripts live one level up. Forward slashes: they locate
# themselves with `dirname "$0"`, which does not split on backslashes.
$scriptsDir = Split-Path -Parent $PSScriptRoot
$script = (Join-Path $scriptsDir $commands[[string]$args[0]]) -replace '\\', '/'
$rest = @()
if ($args.Count -gt 1) { $rest = $args[1..($args.Count - 1)] }
& $bash $script @rest
exit $LASTEXITCODE

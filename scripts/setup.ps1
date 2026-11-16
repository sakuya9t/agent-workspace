<#
.SYNOPSIS
  One-shot bootstrap for a clean Windows machine -- the Windows counterpart of
  scripts/setup.sh.

.DESCRIPTION
  Installs what the Agent Session Manager needs and does a first build:

    1. Git for Windows -- git itself, plus the Git Bash the service scripts
       (start.sh, stop.sh, ...) run under.
    2. Visual Studio 2022 Build Tools with the C++ workload -- the MSVC linker
       and C compiler Rust needs (ring and the bundled SQLite compile C).
    3. Rust via rustup (stable, x86_64-pc-windows-msvc) + clippy and rustfmt.
    4. Node.js LTS -- builds the web client and runs the managed Vite UI.
    5. cargo build of the workspace, then npm ci + npm run build of the client.

  Everything comes from winget and every step is skipped when already
  satisfied, so it is safe to re-run. The Build Tools, Git and Node installers
  ask for elevation (a UAC prompt); rustup installs per user.

.PARAMETER Release
  Build the release profile instead of debug.

.PARAMETER NoBuild
  Install prerequisites only; build nothing.

.PARAMETER NoClient
  Skip the web client (Node install, npm ci, npm run build).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\setup.ps1
#>
[CmdletBinding()]
param(
  [switch]$Release,
  [switch]$NoBuild,
  [switch]$NoClient
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot

function Log([string]$msg)  { Write-Host "[setup] $msg" -ForegroundColor Cyan }
function Warn([string]$msg) { Write-Host "[setup] $msg" -ForegroundColor Yellow }
function Fail([string]$msg) { Write-Host "[setup] $msg" -ForegroundColor Red; exit 1 }

# Installers update the registry PATH, not this session's; re-read it so the
# tools they install are usable in the steps that follow.
function Update-SessionPath {
  $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  $env:Path = "$machine;$user"
  $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
  if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
}

function Have([string]$name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

function Install-WithWinget([string]$id, [string]$what, [string[]]$extra = @()) {
  if (-not (Have 'winget')) {
    Fail "winget is not available. Install '$what' yourself (or get App Installer from the Microsoft Store), then re-run."
  }
  Log "installing $what ($id) via winget..."
  $wingetArgs = @('install', '--id', $id, '-e', '--accept-source-agreements', '--accept-package-agreements') + $extra
  & winget @wingetArgs
  # winget reports "already installed / no newer version" with a non-zero code;
  # the probe after each install is what decides success.
  Update-SessionPath
}

function Find-GitBash {
  $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}, (Join-Path $env:LOCALAPPDATA 'Programs'))
  foreach ($r in $roots) {
    if (-not $r) { continue }
    $candidate = Join-Path $r 'Git\bin\bash.exe'
    if (Test-Path $candidate) { return $candidate }
  }
  $git = Get-Command git -ErrorAction SilentlyContinue
  if ($git) {
    $candidate = Join-Path (Split-Path (Split-Path $git.Source)) 'bin\bash.exe'
    if (Test-Path $candidate) { return $candidate }
  }
  return $null
}

function Test-MsvcTools {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path $vswhere)) { return $false }
  $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  return [bool]$path
}

# --- 1. Git (and Git Bash) ---------------------------------------------------
if ((Have 'git') -and (Find-GitBash)) {
  Log "git present -- $(git --version)"
} else {
  Install-WithWinget 'Git.Git' 'Git for Windows'
  if (-not (Find-GitBash)) { Fail 'Git Bash still not found after installing Git for Windows.' }
}

# --- 2. MSVC build tools -----------------------------------------------------
if (Test-MsvcTools) {
  Log 'MSVC C++ build tools present'
} else {
  Install-WithWinget 'Microsoft.VisualStudio.2022.BuildTools' 'Visual Studio 2022 Build Tools (C++ workload)' @(
    '--override', '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
  )
  if (-not (Test-MsvcTools)) { Fail 'the MSVC C++ tools are still missing after the Build Tools install.' }
}

# --- 3. Rust -----------------------------------------------------------------
Update-SessionPath
if (-not (Have 'cargo')) {
  Install-WithWinget 'Rustlang.Rustup' 'rustup'
  if (-not (Have 'rustup')) { Fail 'rustup still not on PATH after installing it.' }
}
if (-not (Have 'cargo')) {
  Log 'installing the stable Rust toolchain...'
  & rustup default stable-x86_64-pc-windows-msvc
  Update-SessionPath
}
if (-not (Have 'cargo')) { Fail 'cargo still not available after installing Rust.' }
Log "rust present -- $(cargo --version)"
$components = (& rustup component list --installed) -join "`n"
foreach ($c in 'clippy', 'rustfmt') {
  if ($components -notmatch "(?m)^$c") {
    Log "adding rustup component $c"
    & rustup component add $c
  }
}

# --- 4. Node -----------------------------------------------------------------
if (-not $NoClient) {
  if (Have 'node') {
    Log "node present -- $(node --version)"
  } else {
    Install-WithWinget 'OpenJS.NodeJS.LTS' 'Node.js LTS'
    if (-not (Have 'node')) { Fail 'node still not on PATH after installing Node.js.' }
  }
  # Parsed from `node --version` (v26.13.0): passing JS with quotes to a native
  # program is unreliable in Windows PowerShell, which drops embedded quotes.
  $major = [int](((node --version) | Select-Object -First 1) -replace '^v(\d+).*$', '$1')
  if ($major -lt 20) { Warn "Node $(node --version) detected; the client wants Node 20+." }
}

# --- 5. Build ----------------------------------------------------------------
if ($NoBuild) {
  Log '-NoBuild -- skipping the builds'
} else {
  Push-Location $Root
  try {
    $profileName = 'debug'
    $cargoArgs = @('build', '--workspace')
    if ($Release) { $cargoArgs += '--release'; $profileName = 'release' }
    Log "building the workspace ($profileName) -- this can take a few minutes the first time..."
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed.' }
    Log "build ok -- binaries under target\$profileName (asm-daemon, asmux, asm-relay, asm-launch)"

    if (-not $NoClient) {
      Push-Location (Join-Path $Root 'client')
      try {
        Log 'installing web client deps (npm ci)...'
        & npm ci
        if ($LASTEXITCODE -ne 0) { Fail 'npm ci failed.' }
        Log 'building the web client (client\dist)...'
        & npm run build
        if ($LASTEXITCODE -ne 0) { Warn "client build failed -- re-run 'npm run build' in client\ to produce client\dist." }
      } finally { Pop-Location }
    }
  } finally { Pop-Location }
}

Log 'setup complete.'
Write-Host @"

Next steps:
  1. Open a NEW terminal so it picks up the updated PATH.
  2. Start the stack -- from PowerShell or cmd:
         scripts\asm start
     or from Git Bash:
         scripts/start.sh
     Then open the web UI at http://127.0.0.1:5273 (the daemon itself also
     serves the built client at http://127.0.0.1:4600).
  3. Check it and grab this host's enrollment token:
         scripts\asm status
         scripts\asm token
"@
exit 0

# Install j3c, the J3 compiler, on Windows (x64, Windows 10 1903+), from GitHub Releases. No admin rights.
#   irm https://github.com/Jacoby-Y/j3/releases/latest/download/install.ps1 | iex
# With options (a script piped to iex takes none):
#   & ([scriptblock]::Create((irm https://github.com/Jacoby-Y/j3/releases/latest/download/install.ps1))) -Version 0.1.0
# Options (or environment):
#   -Version 0.1.0     ($env:J3_VERSION)      a release instead of the latest
#   -InstallDir DIR    ($env:J3_INSTALL_DIR)  default %LOCALAPPDATA%\Programs\j3c
#   -ZipPath FILE                             a local release zip (CI); its checksum comes from a SHA256SUMS next to it
# Then it adds the folder to the user PATH (if missing) and runs `j3c setup` (clang and the SDL2 kit, about 600 MB in
# %LOCALAPPDATA%\j3). Uninstall: delete %LOCALAPPDATA%\Programs\j3c and %LOCALAPPDATA%\j3, and the PATH entry.
# Works on Windows PowerShell 5.1 and PowerShell 7. Errors throw (never `exit`: under iex that closes the window).
param(
    [string]$Version = $env:J3_VERSION,
    [string]$InstallDir = $env:J3_INSTALL_DIR,
    [string]$ZipPath = ''
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # 5.1's progress bar makes Invoke-WebRequest many times slower
$Repo = 'Jacoby-Y/j3'

if (-not [Environment]::Is64BitOperatingSystem) { throw "install.ps1: j3c needs 64-bit Windows" }
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\j3c' }
# Older 5.1 setups default to TLS 1.0, which GitHub refuses.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('j3c-install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null

# The sha256 for file $name from a SHA256SUMS file ("<hex>  <name>" lines, `*` before binary names allowed).
function Get-ExpectedSum([string]$sums, [string]$name) {
    foreach ($line in Get-Content -LiteralPath $sums) {
        $parts = $line -split '\s+', 2
        if ($parts.Count -eq 2 -and ($parts[1] -eq $name -or $parts[1] -eq "*$name")) { return $parts[0] }
    }
    return $null
}

# One PATH entry in a comparable form: env vars expanded, no trailing `\`, case folded by the caller (-ieq).
function Normalize-PathEntry([string]$p) {
    return [Environment]::ExpandEnvironmentVariables($p.Trim().Trim('"')).TrimEnd('\')
}

try {
    if ($ZipPath) {
        if (-not (Test-Path -LiteralPath $ZipPath -PathType Leaf)) { throw "install.ps1: -ZipPath: no file $ZipPath" }
        $zip = (Resolve-Path -LiteralPath $ZipPath).Path
        $name = Split-Path -Leaf $zip
        $sums = Join-Path (Split-Path -Parent $zip) 'SHA256SUMS'
        if (-not (Test-Path -LiteralPath $sums)) { throw "install.ps1: no SHA256SUMS next to $zip" }
    } else {
        if (-not $Version) {
            $latest = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$Repo/releases/latest"
            $Version = $latest.tag_name
            if (-not $Version) { throw "install.ps1: could not find the latest release of $Repo" }
        }
        $Version = $Version -replace '^v', ''
        $name = "j3c-$Version-x86_64-windows.zip"
        $base = "https://github.com/$Repo/releases/download/v$Version"
        Write-Host "downloading j3c $Version"
        $zip = Join-Path $tmp $name
        $sums = Join-Path $tmp 'SHA256SUMS'
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $zip
        Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums
    }

    # Check before unpacking anything.
    $want = Get-ExpectedSum $sums $name
    if (-not $want) { throw "install.ps1: SHA256SUMS has no line for $name" }
    $got = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash
    if ($got -ine $want) { throw "install.ps1: checksum mismatch for $name (got $got, want $want); not installing" }

    $x = Join-Path $tmp 'x'
    Expand-Archive -LiteralPath $zip -DestinationPath $x
    if (-not (Test-Path -LiteralPath (Join-Path $x 'j3c\j3c.exe'))) { throw "install.ps1: $name has no j3c\j3c.exe" }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item -Path (Join-Path $x 'j3c\*') -Destination $InstallDir -Recurse -Force
    $exe = Join-Path $InstallDir 'j3c.exe'
    Write-Host "installed $exe"

    # The user PATH from the registry, unexpanded (not $env:Path, which has the machine entries too), so entries
    # like %USERPROFILE%\bin stay as they are and the value keeps its REG_EXPAND_SZ kind.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $userPath = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $mine = Normalize-PathEntry $InstallDir
        $have = $false
        foreach ($e in ($userPath -split ';')) {
            if ($e.Trim() -and ((Normalize-PathEntry $e) -ieq $mine)) { $have = $true }
        }
        if (-not $have) {
            $new = if ($userPath.Trim().TrimEnd(';')) { $userPath.TrimEnd(';') + ';' + $InstallDir } else { $InstallDir }
            $key.SetValue('Path', $new, [Microsoft.Win32.RegistryValueKind]::ExpandString)
            # A registry write tells no one; setting a user variable through .NET broadcasts WM_SETTINGCHANGE, so
            # new terminals (and Explorer) see the PATH. The variable is removed again at once.
            [Environment]::SetEnvironmentVariable('J3C_INSTALL_NOTIFY', '1', 'User')
            [Environment]::SetEnvironmentVariable('J3C_INSTALL_NOTIFY', $null, 'User')
            Write-Host "added $InstallDir to your user PATH"
        }
    } finally {
        $key.Close()
    }
    if (-not (($env:Path -split ';') | Where-Object { $_ -and ((Normalize-PathEntry $_) -ieq (Normalize-PathEntry $InstallDir)) })) {
        $env:Path = $env:Path.TrimEnd(';') + ';' + $InstallDir
    }

    & $exe setup
    if ($LASTEXITCODE -ne 0) { throw "install.ps1: ``j3c setup`` failed (exit $LASTEXITCODE); run it again, or ``j3c doctor``" }
    Write-Host ''
    Write-Host 'open a new terminal, then: j3c new hello --app'
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# The Windows CI smoke test (plan 08 chunk 8): install the release zip the way a user would, then build and run
# with the installed j3c.exe. Run from the repo root on windows-latest (pwsh), after the Linux job's artifact:
#   tools/ci/windows-smoke.ps1 -ZipDir artifact      # the folder with j3c-*-x86_64-windows.zip and SHA256SUMS
# Steps: install.ps1 -ZipPath (runs `j3c setup`), doctor, console hello, speculative/01-07 against their Linux
# stdout (j3c-bootstrap/tests/expect/run/), an SDL app with mesa's software opengl32.dll, then the self-build:
# j3c.exe builds j3c-bootstrap and both print the same `version --verbose` (as build.sh checks on Linux).
# Not here: anything on the fork-based paths still off on Windows (parallel jobs/cache, `j3c pkg`, dist pkg tools).
param([Parameter(Mandatory = $true)][string]$ZipDir)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$repo = (Get-Location).Path

# mesa-dist-win: the runner has no GPU and its opengl32.dll is GDI GL 1.1. The digest is GitHub's for the asset.
$MesaUrl = 'https://github.com/pal1000/mesa-dist-win/releases/download/26.2.7/mesa3d-26.2.7-release-mingw.7z'
$MesaSha256 = '19c74aad7b09e682473b8de4c35b47e6498de97b71cae5511a231b8eb02f2b22'

function Step([string]$what) { Write-Host "::group::$what" }
function EndStep { Write-Host '::endgroup::' }
function Fail([string]$msg) { Write-Host "::error::$msg"; throw $msg }
# Run a native command; stdout lines back, a non-zero exit fails unless -AllowFail.
function Native([string]$exe, [string[]]$argv, [switch]$AllowFail) {
    $out = & $exe @argv
    if (-not $AllowFail -and $LASTEXITCODE -ne 0) { Fail "$exe $($argv -join ' '): exit $LASTEXITCODE" }
    return $out
}

$work = Join-Path $env:RUNNER_TEMP 'j3-smoke'
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Path $work | Out-Null

Step 'install.ps1 -ZipPath'
$zip = Get-ChildItem -Path $ZipDir -Filter 'j3c-*-x86_64-windows.zip' | Select-Object -First 1
if (-not $zip) { Fail "no j3c-*-x86_64-windows.zip in $ZipDir" }
& (Join-Path $repo 'install.ps1') -ZipPath $zip.FullName
$j3c = Join-Path $env:LOCALAPPDATA 'Programs\j3c\j3c.exe'
if (-not (Test-Path $j3c)) { Fail "install.ps1 left no $j3c" }
Native $j3c @('version', '--verbose')
EndStep

Step 'j3c doctor (diagnostic)'
Native $j3c @('doctor') -AllowFail
EndStep

# Projects and samples live outside the checkout, so no on-disk stdlib next to them can hide an embedded-name bug.
Step 'console hello'
Push-Location $work
Native $j3c @('new', 'hello') | Out-Null
Push-Location hello
$got = (Native $j3c @('run')) -join "`n"
Pop-Location; Pop-Location
if ($got -ne 'Hello, world!') { Fail "hello printed: $got" }
EndStep

Step 'speculative/01-07 vs Linux'
$spec = Join-Path $work 'spec'
New-Item -ItemType Directory -Path $spec | Out-Null
# -Path, not -Filter: only -Path understands [1-7]. Seven programs, else the step would pass vacuously.
$progs = @(Get-ChildItem -Path (Join-Path $repo 'speculative/0[1-7]_*.j3'))
if ($progs.Count -ne 7) { Fail "speculative/0[1-7]_*.j3: found $($progs.Count) programs, want 7" }
foreach ($f in $progs) {
    $copy = Join-Path $spec $f.Name
    Copy-Item $f.FullName $copy
    $lines = @(Native $j3c @('run', '--headless', '--frames', '3', $copy) -AllowFail)
    $code = $LASTEXITCODE
    # As tests/diff.sh `run` writes them: stdout without J3STATS lines, then `--- exit N`.
    $got = (@($lines | Where-Object { $_ -notmatch '^J3U?I?STATS ' }) + "--- exit $code") -join "`n"
    $want = ((Get-Content -Raw (Join-Path $repo "j3c-bootstrap/tests/expect/run/speculative/$($f.Name).out")) -replace "`r", '').TrimEnd("`n")
    if ($got -ne $want) { Fail "$($f.Name): got`n$got`nwant`n$want" }
    Write-Host "$($f.Name) ok"
}
EndStep

Step 'SDL app (mesa llvmpipe)'
$mesa7z = Join-Path $work 'mesa.7z'
Invoke-WebRequest -UseBasicParsing -Uri $MesaUrl -OutFile $mesa7z
$h = (Get-FileHash -Algorithm SHA256 $mesa7z).Hash
if ($h -ine $MesaSha256) { Fail "mesa: sha256 $h, want $MesaSha256" }
Native '7z' @('x', '-y', "-o$work\mesa", $mesa7z) | Out-Null
Push-Location $work
Native $j3c @('new', 'app', '--app') | Out-Null
Push-Location app
Native $j3c @('build')
# opengl32.dll is looked up beside the exe before System32: `build` writes target\app.exe, `run` target\.j3c\host\.
foreach ($d in @('target', 'target\.j3c\host')) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    Copy-Item "$work\mesa\x64\*.dll" $d
}
# Bounded: a hung game (no frames, a modal box) fails in 5 min with its output instead of eating the job timeout.
$p = Start-Process -FilePath $j3c -ArgumentList 'run', '--headless', '--frames', '5', '--screenshot', 's.png' `
    -NoNewWindow -PassThru -RedirectStandardOutput run.out -RedirectStandardError run.err
$null = $p.Handle  # keeps ExitCode readable after the wait
$hung = -not $p.WaitForExit(300000)
if ($hung) { Stop-Process -Id $p.Id -Force; Get-Process app -ErrorAction SilentlyContinue | Stop-Process -Force }
Get-Content run.out, run.err -ErrorAction SilentlyContinue | Write-Host
if (Test-Path crash.log) { Write-Host '--- crash.log'; Get-Content crash.log | Write-Host }
if ($hung) { Fail 'app: j3c run --headless hung for 5 min' }
if ($p.ExitCode -ne 0) { Fail "app: j3c run exit $($p.ExitCode)" }
if (-not (Test-Path 's.png')) { Fail 'app: no s.png' }
Pop-Location; Pop-Location
EndStep

Step 'self-build: j3c.exe builds j3c-bootstrap'
# The same version file the release build embedded (tools/release/package.sh).
$vf = Join-Path $repo 'j3c-bootstrap/text/version-build.txt'
$d = (git describe --tags --match 'v[0-9]*' --dirty) -replace '^v', ''
$t = (git describe --tags --match 'v[0-9]*' --abbrev=0) -replace '^v', ''
Set-Content -Path $vf -Value "$d`n$t" -NoNewline
try {
    Native $j3c @('build', '--opt', '2', 'j3c-bootstrap')
} finally {
    Remove-Item $vf
}
$built = Join-Path $repo 'j3c-bootstrap\target\j3c.exe'
$v1 = (Native $j3c @('version', '--verbose') | Where-Object { $_ -notmatch '^binary ' }) -join "`n"
$v2 = (Native $built @('version', '--verbose') | Where-Object { $_ -notmatch '^binary ' }) -join "`n"
if ($v1 -ne $v2) { Fail "installed and self-built j3c disagree:`n$v1`n---`n$v2" }
Write-Host 'installed and self-built j3c agree'
EndStep

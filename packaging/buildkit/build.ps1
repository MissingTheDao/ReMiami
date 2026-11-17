# reMiami build kit for Windows: builds the game from the source inside this kit on your computer, then starts it.
# Contains no game data, no reVC source and no prebuilt game. Needs no network and no root.
#
# usage: <kit .exe> --revc-source <reVC copy> --game <Vice City folder> --sdl3 <folder> --mpg123 <folder>
#                  [--ps2-disc <image>] [--xbox-disc <image>] [--dest <dir>] [--jobs N] [--rebuild] [--check]
# Exactly these versions are accepted, so every build plays the same (each is checked by SHA-256):
#   --revc-source  the reVC source (re3 "miami" branch), snapshot dc2147268b17b86656b629d5370bf2e90229e618:
#                  a folder or a .zip/.tar.* archive; every needed file is checked
#   --game         Vice City for PC, the Steam release (gta-vc.exe sha256 8c2318d61bffaaff5e638449543dbf99ea932dca85f297c7190d2efd525b848f)
#   --ps2-disc     optional, for the PS2 style: PS2 US v1.40 (SLUS-20552; boot file sha256 39f5abacc6aa77c4...)
#   --xbox-disc    optional, for the Xbox style: Xbox US (Rev 1) (default.xbe sha256 9367714de9f4aa12...)
#                  (disc image files, ISO or XISO; imported into <game>/userfiles/import)
#   --dest         where to build (default: %LOCALAPPDATA%\remiami\<version>)
#   --jobs N       parallel build jobs (default: half the CPUs)
#   --rebuild      build again even if this version was built before
#   --sdl3         the official SDL3-devel-<version>-VC folder (default: %SDL3_DIR%)
#   --mpg123       mpg123 built with CMake and MSVC as a static library (include\mpg123.h, lib\mpg123.lib; default: %MPG123_DIR%)
#   --check        only check the tools and report what is missing
# Tools: Git for Windows, Visual Studio 2022 Build Tools (C++), CMake and Ninja (the VS copies are fine), Rust 1.99.1 (rustup).
# Once built, run it again with --game <folder> (no --revc-source needed) to play.

$ErrorActionPreference = 'Continue'
$VERSION = (Get-Content "$PSScriptRoot\VERSION" -Raw).Trim()
$PC_SHA256 = "8c2318d61bffaaff5e638449543dbf99ea932dca85f297c7190d2efd525b848f"

$RevCSource = $null
$Game = $null
$Ps2Disc = $null
$XboxDisc = $null
$Dest = $null
$Jobs = $null
$Rebuild = $false
$Check = $false
$Help = $false
$Sdl3 = $null
$Mpg123 = $null

for ($i = 0; $i -lt $args.Length; $i++) {
    $arg = $args[$i]
    switch ($arg) {
        '--revc-source' { $RevCSource = $args[++$i]; continue }
        '--game' { $Game = $args[++$i]; continue }
        '--ps2-disc' { $Ps2Disc = $args[++$i]; continue }
        '--xbox-disc' { $XboxDisc = $args[++$i]; continue }
        '--dest' { $Dest = $args[++$i]; continue }
        '--jobs' { $Jobs = $args[++$i]; continue }
        '--rebuild' { $Rebuild = $true; continue }
        '--check' { $Check = $true; continue }
        { $_ -eq '-h' -or $_ -eq '--help' } {
            foreach ($l in (Get-Content $PSCommandPath)) { if ($l -notmatch '^#') { break }; Write-Host ($l -replace '^# ?', '') }
            exit 0 
        }
        '--sdl3' { $Sdl3 = $args[++$i]; continue }
        '--mpg123' { $Mpg123 = $args[++$i]; continue }
        default { Write-Host "remiami: unknown argument: $arg (see --help)"; exit 2 }
    }
}

if (-not $Dest) { $Dest = "$env:LOCALAPPDATA\remiami\$VERSION" }
if (-not $Jobs) { $Jobs = [Math]::Max(1, [int][Math]::Floor(([Environment]::ProcessorCount + 1) / 2)) }
if (-not $Sdl3) { $Sdl3 = $env:SDL3_DIR }
if (-not $Mpg123) { $Mpg123 = $env:MPG123_DIR }
$Dest = [IO.Path]::GetFullPath($Dest)
$Bin = "$Dest\src\rust\target\release\remiami.exe"
$NeedBuild = $Rebuild -or -not (Test-Path $Bin)

$script:Missing = @()
$script:BashExe = $null; $script:Vcvars = $null; $script:ExtraPath = @()
function Has($Cmd) { [bool](Get-Command $Cmd -ErrorAction SilentlyContinue) }
function CheckTools {
    if (-not (Has git)) { $script:Missing += "Git for Windows (git.exe on PATH): https://git-scm.com" }
    else {
        $GitRoot = Split-Path (Split-Path (Get-Command git).Source)
        $script:BashExe = Join-Path $GitRoot "bin\bash.exe"
        if (-not (Test-Path $script:BashExe)) { $script:Missing += "Git Bash (bin\bash.exe of Git for Windows)" }
        foreach ($t in 'diff', 'patch', 'sha256sum') {
            if (-not (Test-Path (Join-Path $GitRoot "usr\bin\$t.exe"))) { $script:Missing += "$t (comes with Git for Windows)" }
        }
    }
    $VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $VsInstall = $null
    if (Test-Path $VsWhere) {
        $VsInstall = & $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1
    }
    if (-not $VsInstall) { $script:Missing += 'Visual Studio 2022 Build Tools with "Desktop development with C++"' }
    else {
        $script:Vcvars = Join-Path $VsInstall "VC\Auxiliary\Build\vcvars64.bat"
        if (-not (Test-Path $script:Vcvars)) { $script:Missing += "vcvars64.bat of the Visual Studio C++ tools" }
        $VsCmake = Join-Path $VsInstall "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
        $VsNinja = Join-Path $VsInstall "Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja"
        if (-not (Has cmake)) { if (Test-Path "$VsCmake\cmake.exe") { $script:ExtraPath += $VsCmake } else { $script:Missing += "CMake (cmake.exe), or the C++ CMake tools of Visual Studio" } }
        if (-not (Has ninja)) { if (Test-Path "$VsNinja\ninja.exe") { $script:ExtraPath += $VsNinja } else { $script:Missing += "Ninja (ninja.exe), or the C++ CMake tools of Visual Studio" } }
    }
    if (-not (Has rustup) -or -not (Has cargo)) { $script:Missing += "Rust: rustup (https://rustup.rs), then  rustup toolchain install 1.99.1" }
    elseif (-not ((rustup toolchain list 2>$null) | Select-String '^1\.99\.1')) { $script:Missing += "Rust 1.99.1 for rustup: run  rustup toolchain install 1.99.1" }
    if ($NeedBuild -or $Check) {
        if (-not $Sdl3) { $script:Missing += "--sdl3 <folder>: the official SDL3-devel-<version>-VC package, unpacked" }
        elseif (-not ((Test-Path "$Sdl3\include\SDL3\SDL.h") -and (Test-Path "$Sdl3\lib\x64\SDL3.lib") -and (Test-Path "$Sdl3\lib\x64\SDL3.dll"))) { $script:Missing += "--sdl3 ${Sdl3}: not an SDL3-devel VC folder (include\SDL3\SDL.h, lib\x64\SDL3.lib, lib\x64\SDL3.dll)" }
        if (-not $Mpg123) { $script:Missing += "--mpg123 <folder>: mpg123 built with CMake and MSVC as a static library" }
        elseif (-not ((Test-Path "$Mpg123\include\mpg123.h") -and (Test-Path "$Mpg123\lib\mpg123.lib"))) { $script:Missing += "--mpg123 ${Mpg123}: no include\mpg123.h or lib\mpg123.lib" }
    }
}

CheckTools
if ($script:Missing.Count -gt 0) {
    Write-Host "remiami: these tools are missing (nothing was built):"
    $script:Missing | ForEach-Object { Write-Host "  - $_" }
    exit 3
}

if ($Check) {
    Write-Host "remiami: all build tools found"
    exit 0
}

if ($Game) {
    if (-not (Test-Path "$Game\models\gta3.img")) {
        Write-Host "remiami: --game ${Game}: not a Vice City folder (no models/gta3.img)"
        exit 2
    }
    $Game = (Get-Item $Game).FullName
    $Exe = Get-ChildItem $Game -Filter "gta-vc.exe" -File | Select-Object -First 1
    if (-not $Exe -or $Exe.Length -eq 0) {
        Write-Host "remiami: --game ${Game}: not the Steam release of Vice City (gta-vc.exe missing or a different version)."
        Write-Host "remiami: this build accepts only the Steam release, so every build plays the same (see --help)"
        exit 2
    }
    $Hash = (Get-FileHash $Exe.FullName -Algorithm SHA256).Hash.ToLower()
    if ($Hash -ne $PC_SHA256) {
        Write-Host "remiami: --game ${Game}: not the Steam release of Vice City (gta-vc.exe missing or a different version)."
        Write-Host "remiami: this build accepts only the Steam release, so every build plays the same (see --help)"
        exit 2
    }
}

foreach ($d in @($Ps2Disc, $XboxDisc)) {
    if ($d -and -not (Test-Path $d)) {
        Write-Host "remiami: disc image not found: $d"
        exit 2
    }
    if (-not $d -or $Game) { continue }
    Write-Host "remiami: importing a disc needs --game (the import goes into its userfiles folder)"
    exit 2
}

if ($NeedBuild) {
    if (-not $RevCSource) {
        Write-Host "remiami: this version is not built yet: give --revc-source <your reVC copy> (see --help)"
        exit 2
    }
    Write-Host "remiami: building reMiami $VERSION in $Dest (jobs: $Jobs; about 5-15 minutes)"
    New-Item -ItemType Directory -Force -Path "$Dest" | Out-Null
    
    try {
        $ErrorActionPreference = 'Stop'
        if (Test-Path "$Dest\src.new") { Remove-Item "$Dest\src.new" -Recurse -Force }
        robocopy "$PSScriptRoot\src" "$Dest\src.new" /E /NFL /NDL /NJH /NJS /NP > $null
        if ($LASTEXITCODE -ge 8) { throw "robocopy failed ($LASTEXITCODE)" }
        if (Test-Path "$Dest\src\rust\target") { Move-Item "$Dest\src\rust\target" "$Dest\src.new\rust\target" -Force }
        if (Test-Path "$Dest\src") { Remove-Item "$Dest\src" -Recurse -Force }
        Move-Item "$Dest\src.new" "$Dest\src" -Force
    } catch {
        Write-Host "remiami: copying the source failed: $_"
        exit 1
    } finally {
        $ErrorActionPreference = 'Continue'
    }
    
    function Posix($p) {
        $f = [IO.Path]::GetFullPath($p)
        if ($f -match '^([A-Za-z]):(.*)$') { return '/' + $matches[1].ToLower() + ($matches[2] -replace '\\', '/') }
        return $f -replace '\\', '/'
    }
    if (-not (Test-Path $RevCSource)) { Write-Host "remiami: --revc-source ${RevCSource}: not found"; exit 2 }
    $DestPosix = Posix $Dest
    $RevCPosix = Posix $RevCSource
    & $script:BashExe -lc "cd '$DestPosix/src' && scripts/revc-source.sh prepare '$RevCPosix' --force"
    if ($LASTEXITCODE -ne 0) { Write-Host "remiami: the reVC copy was not accepted (see above)"; exit 4 }

    $Sdl3 = [IO.Path]::GetFullPath($Sdl3); $Mpg123 = [IO.Path]::GetFullPath($Mpg123)
    $q = [char]34
    # One line per step in a .cmd file: cmd expands %PATH%/%LIB% per line, so these see what vcvars set.
    $Lines = @("@echo off", "call ${q}$($script:Vcvars)${q} >nul || exit /b 1")
    if ($script:ExtraPath.Count) { $Lines += "set ${q}PATH=" + ($script:ExtraPath -join ';') + ";%PATH%${q}" }
    $Lines += @(
        "set ${q}MPG123_DIR=$Mpg123${q}",
        "set ${q}LIB=$Sdl3\lib\x64;%LIB%${q}",
        "set ${q}CARGO_NET_OFFLINE=true${q}",
        "set ${q}CARGO_BUILD_JOBS=$Jobs${q}",
        "set ${q}NUM_JOBS=$Jobs${q}",
        "set ${q}CMAKE_BUILD_PARALLEL_LEVEL=$Jobs${q}",
        "cd /d ${q}$Dest\src\rust${q} || exit /b 1",
        "cargo build --release --locked -p revc")
    $BuildCmd = "$Dest\build-remiami.cmd"
    Set-Content -Path $BuildCmd -Value $Lines -Encoding Ascii
    cmd.exe /d /c "${q}$BuildCmd${q}"
    if ($LASTEXITCODE -ne 0) { Write-Host "remiami: the build failed (see above)"; exit 5 }
    
    Copy-Item "$Sdl3\lib\x64\SDL3.dll" "$Dest\src\rust\target\release\" -Force
}

if (-not $Game) {
    Write-Host "remiami: built: $Bin. To play: run the kit again with --game <your Vice City folder>"
    exit 0
}

foreach ($d in @($Ps2Disc, $XboxDisc)) {
    if (-not $d) { continue }
    Write-Host "remiami: importing $d (only PS2 US v1.40 / Xbox US Rev 1 are accepted)"
    Set-Location $Game
    $Env:REVC_GAME_DIR = $Game
    $Env:REVC_ASSETS_DIR = "$Dest\src\assets"
    $AbsDisc = (Get-Item $d).FullName
    & $Bin --import $AbsDisc
    if ($LASTEXITCODE -ne 0) { Write-Host "remiami: the disc was not imported (see above)"; exit 6 }
}

Write-Host "remiami: starting reMiami $VERSION with $Game"
Set-Location $Game
$Env:REVC_GAME_DIR = $Game
$Env:REVC_ASSETS_DIR = "$Dest\src\assets"
& $Bin
exit $LASTEXITCODE

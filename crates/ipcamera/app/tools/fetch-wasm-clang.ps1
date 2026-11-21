# Fetches the clang `nsh run --target web` needs on Windows into tools\llvm, and points cargo at it:
# the web runtime includes zstd-sys, which is C, and cl.exe cannot compile to WebAssembly.

$ErrorActionPreference = 'Stop'

# Change a version and its hash together; zstd publishes no hash, so hash its zip after checking it.
$version = '23.1.5'
$sha256 = 'b2be33385ddc86e83511417a060de927ae8b09d1f3017cc3cdce5c73cd5029d7'
$zstdVersion = '1.5.7'
$zstdSha256 = 'acb4e8111511749dc7a3ebedca9b04190e37a17afeb73f55d4425dbf0b90fad9'

$name = "clang+llvm-$version-x86_64-pc-windows-msvc"
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/clang%2Bllvm-$version-x86_64-pc-windows-msvc.tar.zst"
$zstdName = "zstd-v$zstdVersion-win64"
$zstdUrl = "https://github.com/meta/zstd/releases/download/v$zstdVersion/$zstdName.zip"

$dest = Join-Path $PSScriptRoot 'llvm'
$stamp = Join-Path $dest 'VERSION'
$config = Join-Path (Split-Path $PSScriptRoot -Parent) '.cargo\config.toml'

# Not committed, since other systems need none of it; written without the byte order mark
# Windows PowerShell's own writers add.
function Write-CargoConfig {
    New-Item -ItemType Directory -Force (Split-Path $config -Parent) | Out-Null
    $text = @'
# Written by tools/fetch-wasm-clang.ps1, and ignored by git. `nsh run --target web` builds the web
# runtime from source because this app has a Rust crate, and that runtime includes zstd-sys, which
# is C. cl.exe cannot compile to WebAssembly, so these point cc-rs at the clang the script fetched.
# `relative` resolves them from this crate. A real environment variable wins for CC. AR has to be
# forced: the build finds it already set to plain `llvm-ar`, apparently by nsh, and that is not on
# PATH.
[env]
CC_wasm32_unknown_unknown = { value = "tools/llvm/bin/clang.exe", relative = true, force = false }
AR_wasm32_unknown_unknown = { value = "tools/llvm/bin/llvm-ar.exe", relative = true, force = true }
'@
    [System.IO.File]::WriteAllText($config, $text.Replace("`r`n", "`n"))
    Write-Output "Cargo points at it in $config"
}

if ((Test-Path $stamp) -and ((Get-Content $stamp) -eq $version)) {
    Write-Output "clang $version is already in $dest"
    Write-CargoConfig
    return
}

# Windows' own tools rather than whatever PATH finds first.
$curl = Join-Path $env:WINDIR 'System32\curl.exe'
$tar = Join-Path $env:WINDIR 'System32\tar.exe'

# Kept until the end, so a failed unpack doesn't download again.
$work = Join-Path $env:TEMP "ipcamera-llvm-$version"
New-Item -ItemType Directory -Force $work | Out-Null

function Get-Verified($from, $file, $hash) {
    if ((Test-Path $file) -and ((Get-FileHash $file -Algorithm SHA256).Hash -eq $hash)) { return }
    & $curl --fail --location --retry 3 --silent --show-error --output $file $from
    if ($LASTEXITCODE -ne 0) { throw "Download failed: $from" }
    if ((Get-FileHash $file -Algorithm SHA256).Hash -ne $hash) {
        Remove-Item $file -Force
        throw "$(Split-Path $file -Leaf) does not match its pinned SHA-256, so it was deleted."
    }
}

# Windows' tar cannot read LLVM's archive (a 1 GiB zstd window), so zstd.exe unpacks it.
Write-Output "Downloading zstd $zstdVersion and clang $version (266 MiB)..."
$zstdZip = Join-Path $work "$zstdName.zip"
Get-Verified $zstdUrl $zstdZip $zstdSha256
Get-Verified $url (Join-Path $work "$name.tar.zst") $sha256
& $tar -xf $zstdZip -C $work "$zstdName/zstd.exe"
if ($LASTEXITCODE -ne 0) { throw "Unpacking $zstdZip failed." }

# Only the compiler, the archiver and clang's headers. The pipe runs in cmd because Windows
# PowerShell's pipe passes text, which would corrupt the archive.
$unpacked = Join-Path $work 'unpacked'
if (Test-Path $unpacked) { Remove-Item $unpacked -Recurse -Force }
New-Item -ItemType Directory -Force $unpacked | Out-Null
Write-Output 'Unpacking...'
Push-Location $work
try {
    cmd /c "$zstdName\zstd.exe -d --long=30 -q -c $name.tar.zst | $tar -xf - -C unpacked $name/bin/clang.exe $name/bin/llvm-ar.exe $name/lib/clang/*/include"
    if ($LASTEXITCODE -ne 0) { throw 'Unpacking clang failed.' }
} finally {
    Pop-Location
}

if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Move-Item (Join-Path $unpacked $name) $dest
Set-Content $stamp $version
Remove-Item $work -Recurse -Force

& (Join-Path $dest 'bin\clang.exe') --version | Select-Object -First 1
Write-Output "Installed in $dest"
Write-CargoConfig

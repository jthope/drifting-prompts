# Installs the clang that `nsh run --target web` needs, into tools\llvm beside this script.
#
# An app with its own Rust crate has its web runtime built from source, and that runtime includes
# zstd-sys, which is C. cl.exe cannot compile to WebAssembly, so cc-rs needs clang. The compiler is
# too large to commit, so this fetches one pinned release, and `[env]` in ..\.cargo\config.toml
# points cargo at it. Nothing is installed system-wide and PATH is left alone.
#
# Run it once per machine. It does nothing if this version is already there:
#
#   powershell -ExecutionPolicy Bypass -File tools\fetch-wasm-clang.ps1

$ErrorActionPreference = 'Stop'

# To update, change a version and its hash together. clang's hash is the .tar.zst asset's digest on
# the LLVM release page. zstd's releases publish none, so hash the zip after checking it yourself.
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
if ((Test-Path $stamp) -and ((Get-Content $stamp) -eq $version)) {
    Write-Output "clang $version is already in $dest"
    return
}

# Windows' own tools rather than whatever PATH finds first.
$curl = Join-Path $env:WINDIR 'System32\curl.exe'
$tar = Join-Path $env:WINDIR 'System32\tar.exe'

# Kept until the end, so a failed unpack does not download 264 MiB again.
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

# Windows' tar cannot read LLVM's archive: it was compressed with a 1 GiB window and tar stops at
# 128 MiB. The official zstd.exe can, so it is fetched too, first because it is small, and it goes
# when the temporary folder does.
Write-Output "Downloading zstd $zstdVersion and clang $version (266 MiB)..."
$zstdZip = Join-Path $work "$zstdName.zip"
Get-Verified $zstdUrl $zstdZip $zstdSha256
Get-Verified $url (Join-Path $work "$name.tar.zst") $sha256
& $tar -xf $zstdZip -C $work "$zstdName/zstd.exe"
if ($LASTEXITCODE -ne 0) { throw "Unpacking $zstdZip failed." }

# Only what compiling C to WebAssembly uses: the compiler, the archiver and clang's own headers.
# zstd-sys brings its own libc headers (wasm-shim), so no sysroot is needed. The pipe runs in cmd
# because Windows PowerShell passes data between programs as text, which would corrupt the archive.
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

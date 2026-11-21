[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$out = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $out) { throw "Choose a fresh output directory: $out" }
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw 'Run from an x64 MSVC developer shell.' }
New-Item -ItemType Directory -Path $out | Out-Null
Push-Location -LiteralPath $out
try {
    & cl.exe /nologo /EHsc /W4 /WX /O2 /std:c++17 /Fe:Dx11CopyBenchmark.exe /Fo:benchmark.obj "$PSScriptRoot\benchmark.cpp" /link d3d11.lib dxgi.lib
    if ($LASTEXITCODE -ne 0) { throw "Benchmark build failed with exit code $LASTEXITCODE." }
    $sha = [Security.Cryptography.SHA256]::Create()
    $file = [IO.File]::OpenRead((Join-Path $out 'Dx11CopyBenchmark.exe'))
    try { $hash = [BitConverter]::ToString($sha.ComputeHash($file)).Replace('-', '') }
    finally { $file.Dispose(); $sha.Dispose() }
    Write-Output "SHA256: $hash"
} finally {
    Pop-Location
}

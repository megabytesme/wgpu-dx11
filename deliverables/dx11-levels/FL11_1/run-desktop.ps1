[CmdletBinding()]
param(
    [string]$HalTestExe,
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    [Parameter(Mandatory)][ValidateSet('x64', 'x86')][string]$Architecture,
    [string]$PublicTestExe,
    [string]$SceneTestExe,
    [string]$CoordinationDirectory = 'C:/dev/personal/HorizonCodexCoord-Nov2026'
)

$ErrorActionPreference = 'Stop'
$halExe = if ($HalTestExe) { (Resolve-Path -LiteralPath $HalTestExe).Path }
$publicExe = if ($PublicTestExe) { (Resolve-Path -LiteralPath $PublicTestExe).Path }
$sceneExe = if ($SceneTestExe) { (Resolve-Path -LiteralPath $SceneTestExe).Path }
if (-not ($halExe -or $publicExe -or $sceneExe)) { throw 'Pass at least one test executable.' }
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
New-Item -ItemType Directory -Path $evidence -Force | Out-Null

function Invoke-TestBinary([string]$Exe, [string[]]$Arguments, [string]$Case) {
    $out = Join-Path $evidence "$Case-$Architecture.txt"
    $err = Join-Path $evidence "$Case-$Architecture.stderr.txt"
    $process = Start-Process -FilePath $Exe -ArgumentList $Arguments -Wait -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $out -RedirectStandardError $err
    $stdout = Get-Content -LiteralPath $out -Raw
    $stdout | Write-Host
    if ($process.ExitCode -ne 0 -or $stdout -notmatch 'test result: ok\. [1-9][0-9]* passed') {
        Get-Content -LiteralPath $err | Write-Host
        throw "$Case on $Architecture failed (exit $($process.ExitCode))."
    }
}

function Measure-Scene([string]$Exe) {
    $levels = @('11_1', '11_0', '9_1')
    $samples = foreach ($round in 0..4) {
        foreach ($position in 0..2) {
            $level = $levels[($round + $position) % $levels.Count]
            $env:WGPU_DX11_FORCE_FEATURE_LEVEL = $level
            $out = Join-Path $evidence "scene-$Architecture-$level-$round.txt"
            $err = Join-Path $evidence "scene-$Architecture-$level-$round.stderr.txt"
            $peak = 0L
            $process = Start-Process -FilePath $Exe -ArgumentList @('--nocapture') -PassThru `
                -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err
            try {
                do {
                    try {
                        $process.Refresh()
                        $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
                    }
                    catch [InvalidOperationException] {
                        if (-not $process.HasExited) { throw }
                    }
                } while (-not $process.WaitForExit(5))
                $text = Get-Content -LiteralPath $out -Raw
                if ($process.ExitCode -ne 0 -or $text -notmatch 'test result: ok\. 1 passed' -or
                    $text -notmatch "actual=FL$level " -or $text -notmatch 'submit_to_mapped_us=([0-9]+)') {
                    throw "Scene benchmark at FL$level did not verify a rendered frame."
                }
                [ordered]@{
                    round = $round
                    requested = $level
                    actual = $level
                    submit_to_mapped_us = [int]$Matches[1]
                    peak_process_working_set_bytes = if ($peak -gt 0) { $peak } else { $null }
                    log = Split-Path -Leaf $out
                    log_sha256 = (Get-FileHash -LiteralPath $out -Algorithm SHA256).Hash.ToLowerInvariant()
                }
            }
            finally { $process.Dispose() }
        }
    }
    $samples | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence "bench-$Architecture.json") -Encoding UTF8
    $env:WGPU_DX11_FORCE_FEATURE_LEVEL = '11_1'
}

$lockPath = Join-Path $CoordinationDirectory 'locks/desktop-gpu.lock'
$lock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
$originalExact = $env:WGPU_DX11_FORCE_FEATURE_LEVEL
$originalLegacy = $env:WGPU_DX11_FORCE_FL9_1
$originalRequired = $env:WGPU_DX11_REQUIRE_HARDWARE
try {
    $record = [ordered]@{
        owner = 'fl11_1'
        host = $env:COMPUTERNAME
        pid = $PID
        utc = [DateTime]::UtcNow.ToString('o')
        action = "Exact FL11_1 offscreen D3D11 native and public tests on $Architecture"
    }
    $recordBytes = [Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Compress))
    $lock.Write($recordBytes, 0, $recordBytes.Length)
    $lock.Flush()
    $env:WGPU_DX11_REQUIRE_HARDWARE = '1'
    $env:WGPU_DX11_FORCE_FL9_1 = $null
    $env:WGPU_DX11_FORCE_FEATURE_LEVEL = '11_1'
    if ($sceneExe) { Measure-Scene $sceneExe }
    if ($publicExe) {
        Invoke-TestBinary $publicExe @('tier_fl11_1', '--ignored', '--test-threads=1', '--nocapture') 'public'
        $env:WGPU_DX11_FORCE_FEATURE_LEVEL = $null
        $env:WGPU_DX11_FORCE_FL9_1 = '1'
        Invoke-TestBinary $publicExe @('--test-threads=1', '--nocapture') 'public-fl9_1'
    }
    if ($halExe) {
        Invoke-TestBinary $halExe @('tier_fl11_1', '--ignored', '--test-threads=1', '--nocapture') 'native'
    }
}
finally {
    $env:WGPU_DX11_FORCE_FEATURE_LEVEL = $originalExact
    $env:WGPU_DX11_FORCE_FL9_1 = $originalLegacy
    $env:WGPU_DX11_REQUIRE_HARDWARE = $originalRequired
    $lock.Dispose()
    Remove-Item -LiteralPath $lockPath
}

#Requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('x64','x86')][string]$Architecture,
    [Parameter(Mandatory)][string]$NativeExecutable,
    [string]$SceneExecutable,
    [string]$Coordination = 'C:/dev/personal/HorizonCodexCoord-Nov2026',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'logs'),
    [ValidateRange(0, 3600)][int]$WaitSeconds = 0
)

$ErrorActionPreference = 'Stop'
$candidate = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'source.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

function Invoke-Readback(
    [string]$Executable, [string]$Name, [string]$Level,
    [string]$Test, [switch]$Benchmark, [switch]$AllowUnavailable
) {
    $exe = (Resolve-Path -LiteralPath $Executable).Path
    $log = Join-Path $OutputDirectory "$Name-$Architecture.log"
    $stderr = Join-Path ([IO.Path]::GetTempPath()) ("dx11-fl10-stderr-" + [guid]::NewGuid())
    $testEnvironment = @{
        WGPU_DX11_REQUIRE_HARDWARE = if ($AllowUnavailable) { '0' } else { '1' }
        WGPU_DX11_FORCE_FL9_1 = $null
        WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL = if ($Level) { $Level } else { $null }
        WGPU_DX11_SCENE_BENCH = if ($Benchmark) { '1' } else { $null }
    }
    $testArguments = @('--nocapture','--test-threads=1')
    if ($Test) { $testArguments += @('--exact',$Test) }
    try {
        $process = Start-Process -FilePath $exe -ArgumentList $testArguments `
            -Environment $testEnvironment -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $log -RedirectStandardError $stderr
        $peak = 0
        while (-not $process.WaitForExit(20)) {
            $process.Refresh()
            try {
                $peak = [Math]::Max($peak, $process.PeakWorkingSet64)
            } catch {
                if (-not $process.HasExited) { throw }
            }
        }
        $errors = Get-Content -LiteralPath $stderr -Raw
        if ($errors) { Add-Content -LiteralPath $log -Value $errors }
        Add-Content -LiteralPath $log -Value "Candidate tree=$($candidate.candidate_tree)"
        Add-Content -LiteralPath $log -Value "Executable SHA256=$((Get-FileHash -LiteralPath $exe).Hash)"
        Add-Content -LiteralPath $log -Value "Peak process working set observed while running (bytes)=$peak"
        Get-Content -LiteralPath $log | Out-Host
        if ($process.ExitCode -ne 0) {
            Write-Warning "$Name on $Architecture failed with exit code $($process.ExitCode)."
        }
        return $process.ExitCode -eq 0
    } finally {
        if (Test-Path -LiteralPath $stderr) { Remove-Item -LiteralPath $stderr }
    }
}

$lock = Join-Path $Coordination 'locks/desktop-gpu.lock'
$deadline = [datetime]::UtcNow.AddSeconds($WaitSeconds)
while ($true) {
    try {
        $stream = [IO.File]::Open($lock, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        break
    } catch [IO.IOException] {
        if ([datetime]::UtcNow -ge $deadline -or -not (Test-Path -LiteralPath $lock)) { throw }
        Start-Sleep -Milliseconds 700
    }
}
$session = [guid]::NewGuid().ToString()
try {
    $metadata = @{
        owner = 'fl10_0'
        session = $session
        host = $env:COMPUTERNAME
        pid = $PID
        utc = [datetime]::UtcNow.ToString('o')
        action = "exact FL10_0 native and portable readback ($Architecture)"
    } | ConvertTo-Json -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($metadata)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush()
    $passed = Invoke-Readback $NativeExecutable 'native'
    if ($SceneExecutable) {
        $scene = 'public_dx11_textured_depth_tested_3d_scene_at_fl91'
        $mrt = 'public_fl10_eight_color_targets_with_shared_blend_and_independent_masks'
        $passed = (Invoke-Readback $SceneExecutable 'public-fl10-mrt' -Level '10_0' -Test $mrt) -and $passed
        $passed = (Invoke-Readback $SceneExecutable 'public-default' -Test $scene) -and $passed
        foreach ($level in '9_3','10_1') {
            $name = "public-unsupported-$level"
            $passed = (Invoke-Readback $SceneExecutable $name -Level $level -Test $scene -AllowUnavailable) -and $passed
            $negativeLog = Join-Path $OutputDirectory "$name-$Architecture.log"
            if (-not (Select-String -LiteralPath $negativeLog -SimpleMatch 'found no physical adapter')) {
                Write-Warning "Unsupported selector $level exposed a D3D11 adapter."
                $passed = $false
            }
        }
        $samples = @{'9_1' = 0; '10_0' = 0}
        foreach ($level in '9_1','10_0','10_0','9_1') {
            $samples[$level]++
            $sample = "r$($samples[$level])"
            $passed = (Invoke-Readback $SceneExecutable "scene-$level-$sample" -Level $level -Test $scene -Benchmark) -and $passed
        }
    }
    if (-not $passed) { throw "One or more $Architecture tests failed. See $OutputDirectory." }
} finally {
    $stream.Dispose()
    if ((Get-Content -LiteralPath $lock -Raw | ConvertFrom-Json).session -eq $session) {
        Remove-Item -LiteralPath $lock
    }
}

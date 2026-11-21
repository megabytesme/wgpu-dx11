[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Runner,
    [Parameter(Mandatory)][string]$CtsCheckout,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$CoordinationDirectory = 'C:/dev/personal/HorizonCodexCoord-Nov2026',
    [ValidateRange(0, 300)][int]$GpuWaitSeconds = 0
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$pinned = (Get-Content -LiteralPath (Join-Path $repo 'cts_runner/revision.txt')).Trim()
$revision = & git -C $CtsCheckout rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $revision -ne $pinned -or @(git -C $CtsCheckout status --porcelain).Count) {
    throw "CTS must be a clean checkout at $pinned."
}
$out = New-Item -ItemType Directory -Path $OutputDirectory
$runnerPath = (Resolve-Path -LiteralPath $Runner).Path
$env:DENO_WEBGPU_BACKEND = 'dx11'
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:WGPU_DX11_EXACT_FEATURE_LEVEL = '11_0'
Remove-Item Env:WGPU_DX11_FORCE_FL9_1 -ErrorAction SilentlyContinue
$selectors = [ordered]@{
    'command-buffer-basic' = 'webgpu:api,operation,command_buffer,basic:*'
    'workgroup-size-z' = 'webgpu:api,validation,capability_checks,limits,maxComputeWorkgroupSizeZ:validate,*'
    'workgroups-per-dimension' = 'webgpu:api,validation,capability_checks,limits,maxComputeWorkgroupsPerDimension:validate,*'
    'storage-buffer-binding-size' = 'webgpu:api,validation,capability_checks,limits,maxStorageBufferBindingSize:validate,*'
}

function Read-CtsSummaryCount([string]$Log, [string]$Label) {
    $pattern = '^' + $Label + '\s*=\s*(\d+)\s*/'
    $summary = @(Select-String -LiteralPath $Log -Pattern $pattern)
    if ($summary.Count -ne 1) { return $null }
    [int]$summary[0].Matches[0].Groups[1].Value
}

$runs = [Collections.Generic.List[object]]::new()
Push-Location -LiteralPath $CtsCheckout
$lockPath = Join-Path $CoordinationDirectory 'locks/desktop-gpu.lock'
$lock = $null
try {
    foreach ($entry in $selectors.GetEnumerator()) {
        $listPath = Join-Path $out "$($entry.Key)-list.txt"
        & $runnerPath './tools/run_deno' --list $entry.Value 2>&1 | Set-Content -LiteralPath $listPath
        if ($LASTEXITCODE -ne 0 -or (Get-Item -LiteralPath $listPath).Length -eq 0) {
            throw "Failed to list $($entry.Key). See $listPath."
        }
    }
    $deadline = [DateTime]::UtcNow.AddSeconds($GpuWaitSeconds)
    while (-not $lock) {
        try {
            $lock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        } catch [IO.IOException] {
            if (-not (Test-Path -LiteralPath $lockPath) -or [DateTime]::UtcNow -ge $deadline) { throw }
            Start-Sleep -Seconds 2
        }
    }
    $lockRecord = @{ owner = 'fl11_0'; pid = $PID; host = $env:COMPUTERNAME; utc = [DateTime]::UtcNow.ToString('o'); action = 'Exact FL11_0 CTS' }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($lockRecord | ConvertTo-Json))
    $lock.Write($bytes, 0, $bytes.Length)
    $lock.Flush()
    foreach ($entry in $selectors.GetEnumerator()) {
        $log = Join-Path $out "$($entry.Key).txt"
        $listPath = Join-Path $out "$($entry.Key)-list.txt"
        & $runnerPath './tools/run_deno' --verbose $entry.Value 2>&1 |
            ForEach-Object { $_.ToString() } |
            Set-Content -LiteralPath $log
        $code = $LASTEXITCODE
        $runs.Add(@{
            id = $entry.Key
            command = @($runnerPath, './tools/run_deno', '--verbose', $entry.Value)
            exit_code = $code
            log = Split-Path -Leaf $log
            log_sha256 = (Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()
            list = Split-Path -Leaf $listPath
            list_sha256 = (Get-FileHash -LiteralPath $listPath).Hash.ToLowerInvariant()
            listed = @(Select-String -LiteralPath $listPath -Pattern '^webgpu:').Count
            passed = Read-CtsSummaryCount $log 'Passed\s+w/o warnings'
            warned = Read-CtsSummaryCount $log 'Passed with warnings'
            skipped = Read-CtsSummaryCount $log 'Skipped'
            failed = Read-CtsSummaryCount $log 'Failed'
            exact_hardware = [bool](Select-String -LiteralPath $log -SimpleMatch 'D3D11 strict exact hardware selection: FL11_0')
        })
    }
} finally {
    if ($lock) {
        $lock.Dispose()
        [IO.File]::Delete($lockPath)
    }
    @{
        source_sha = & git -C $repo rev-parse HEAD
        source_changes = @(& git -C $repo status --porcelain)
        cts_sha = $revision
        requested = '11_0'
        hardware_required = $true
        runner_sha256 = (Get-FileHash -LiteralPath $runnerPath).Hash.ToLowerInvariant()
        results = $runs
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'results.json') -Encoding utf8
    Pop-Location
}
if ($runs.Count -ne $selectors.Count -or @($runs | Where-Object {
    $_.exit_code -ne 0 -or -not $_.exact_hardware -or $_.passed -ne $_.listed -or
    $_.failed -ne 0 -or $_.warned -ne 0 -or $_.skipped -ne 0
}).Count -gt 0) {
    throw "CTS coverage is incomplete. See $out."
}

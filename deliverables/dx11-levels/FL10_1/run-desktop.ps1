[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Log,
    [string]$CoordinationPath = 'C:/dev/personal/HorizonCodexCoord-Nov2026',
    [string]$Filter = 'dx11::exact::tests::',
    [switch]$RunRegularTests,
    [ValidateRange(0, 60)][int]$WaitForLockSeconds = 0
)

$ErrorActionPreference = 'Stop'
$exePath = (Resolve-Path -LiteralPath $Executable).Path
$logPath = [IO.Path]::GetFullPath($Log)
if (Test-Path -LiteralPath $logPath) {
    throw "Choose a new log path: $logPath"
}
$lockPath = Join-Path $CoordinationPath 'locks/desktop-gpu.lock'
$wait = [Diagnostics.Stopwatch]::StartNew()
while ($true) {
    try {
        $lock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        break
    } catch {
        if (-not (Test-Path -LiteralPath $lockPath) -or $wait.Elapsed.TotalSeconds -ge $WaitForLockSeconds) {
            throw
        }
        Start-Sleep -Milliseconds 500
    }
}
try {
    $record = [ordered]@{
        owner = 'fl10_1'
        host = $env:COMPUTERNAME
        pid = $PID
        utc = (Get-Date).ToUniversalTime().ToString('o')
        action = "D3D11 hardware tests: $Filter"
    }
    $metadata = [Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Compress))
    $lock.Write($metadata, 0, $metadata.Length)
    $lock.Flush()

    $testArgs = @($Filter, '--nocapture', '--test-threads=1')
    if (-not $RunRegularTests) {
        $testArgs += '--ignored'
    }
    & $exePath @testArgs 2>&1 | Tee-Object -FilePath $logPath
    $testExitCode = $LASTEXITCODE
} finally {
    $lock.Dispose()
    Remove-Item -LiteralPath $lockPath
}
exit $testExitCode

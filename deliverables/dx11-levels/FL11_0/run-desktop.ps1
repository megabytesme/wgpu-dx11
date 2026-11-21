[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$ScratchDirectory,
    [ValidateSet('x64', 'x86')][string]$Architecture = 'x64',
    [string]$CoordinationDirectory = 'C:/dev/personal/HorizonCodexCoord-Nov2026',
    [string]$Msvc = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207',
    [string]$Sdk = 'C:/Program Files (x86)/Windows Kits/10',
    [string]$SdkVersion = '10.0.19041.0',
    [ValidateRange(0, 300)][int]$GpuWaitSeconds = 0,
    [switch]$IncludeFl91Public,
    [switch]$IncludeBenchmarks,
    [switch]$BuildOnly
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$out = New-Item -ItemType Directory -Path $OutputDirectory
$scratch = New-Item -Force -ItemType Directory -Path $ScratchDirectory
$temporary = New-Item -Force -ItemType Directory -Path (Join-Path $scratch 'tmp')
$env:TEMP = $temporary.FullName
$env:TMP = $env:TEMP
$env:CARGO_TARGET_DIR = Join-Path $scratch.FullName 'target'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
$env:Path = "$Msvc/bin/Hostx64/x64;$Sdk/bin/$SdkVersion/x64;$env:Path"
$env:LIB = "$Msvc/lib/x64;$Sdk/Lib/$SdkVersion/um/x64;$Sdk/Lib/$SdkVersion/ucrt/x64"
$targetOptions = @()
if ($Architecture -eq 'x86') {
    $triple = 'i686-pc-windows-msvc'
    $paths = @("$Msvc/lib/x86", "$Sdk/Lib/$SdkVersion/um/x86", "$Sdk/Lib/$SdkVersion/ucrt/x86")
    $flags = @($paths | ForEach-Object { "-Lnative=$_" }) | ConvertTo-Json -Compress
    $targetOptions = @('--target', $triple, '--config', "target.$triple.rustflags=$flags")
}

$results = [Collections.Generic.List[object]]::new()
$builds = [Collections.Generic.List[object]]::new()
function Build-Tests([string]$Label, [string[]]$CargoOptions) {
    $arguments = @('test', '--locked', '--offline', '--no-run', '--message-format=json') +
        $targetOptions + $CargoOptions
    $log = Join-Path $out "$Label-build.jsonl"
    $messages = & cargo @arguments | Tee-Object -FilePath $log
    $code = $LASTEXITCODE
    $builds.Add(@{ suite = $Label; command = @('cargo') + $arguments; exit_code = $code })
    if ($code -ne 0) {
        $messages | ForEach-Object { ($_ | ConvertFrom-Json).message.rendered } | Out-Host
        throw "Compilation failed for $Label. See $log."
    }
    $executables = @($messages | ForEach-Object {
        $item = $_ | ConvertFrom-Json
        if ($item.reason -eq 'compiler-artifact' -and $item.profile.test -and $item.executable) {
            $item.executable
        }
    })
    if ($executables.Count -ne 1) { throw "Expected exactly one $Label test executable." }
    $executables[0]
}

function Run-Tests([string]$Label, [string]$Binary, [string]$Filter) {
    $arguments = @('--nocapture', '--test-threads=1')
    if ($Filter) { $arguments += $Filter }
    $log = Join-Path $out "$Label.txt"
    & $Binary @arguments 2>&1 | ForEach-Object { $_.ToString() } | Tee-Object -FilePath $log | Out-Host
    $code = $LASTEXITCODE
    $counts = @(Select-String -LiteralPath $log -Pattern 'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored')
    $match = if ($counts.Count -eq 1) { $counts[0].Matches[0] }
    $passed = if ($match) { [int]$match.Groups[1].Value } else { 0 }
    $failed = if ($match) { [int]$match.Groups[2].Value } else { 0 }
    $ignored = if ($match) { [int]$match.Groups[3].Value } else { 0 }
    $skipped = @(Select-String -LiteralPath $log -Pattern 'SKIP:').Count
    $results.Add(@{
        suite = $Label
        command = @($Binary) + $arguments
        requested = $env:WGPU_DX11_EXACT_FEATURE_LEVEL
        force_fl9_1 = $env:WGPU_DX11_FORCE_FL9_1
        exit_code = $code
        passed = $passed
        failed = $failed
        ignored = $ignored
        skipped = $skipped
        binary_sha256 = (Get-FileHash -LiteralPath $Binary).Hash.ToLowerInvariant()
        log = Split-Path -Leaf $log
        log_sha256 = (Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()
    })
}

Push-Location -LiteralPath $repo
$lock = $null
try {
    $revision = & git rev-parse HEAD
    $dirty = @(& git status --porcelain)
    $toolchain = & rustc --version
    $hal = Build-Tests 'hal' @('-p', 'wgpu-hal', '--lib', '--no-default-features', '--features', 'dx11-fl11_0')
    $public = Build-Tests 'exact' @('-p', 'wgpu', '--no-default-features', '--features', 'dx11-fl11_0,wgsl,std', '--test', 'dx11_fl11_0')
    if ($IncludeFl91Public) {
        $fl91 = Build-Tests 'fl91-public' @('-p', 'wgpu', '--no-default-features', '--features', 'dx11,wgsl,std', '--test', 'dx11_public')
    }
    if ($IncludeBenchmarks) {
        $scene = Build-Tests 'scene' @('-p', 'wgpu', '--no-default-features', '--features', 'dx11,wgsl,std', '--test', 'dx11_fl91_scene')
    }
    if ($BuildOnly) { return }
    $lockPath = Join-Path $CoordinationDirectory 'locks/desktop-gpu.lock'
    $deadline = [DateTime]::UtcNow.AddSeconds($GpuWaitSeconds)
    while (-not $lock) {
        try {
            $lock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        } catch [IO.IOException] {
            if (-not (Test-Path -LiteralPath $lockPath) -or [DateTime]::UtcNow -ge $deadline) { throw }
            Start-Sleep -Seconds 2
        }
    }
    $lockRecord = @{ owner = 'fl11_0'; pid = $PID; host = $env:COMPUTERNAME; utc = [DateTime]::UtcNow.ToString('o'); action = "$Architecture desktop readback and selection" }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($lockRecord | ConvertTo-Json))
    $lock.Write($bytes, 0, $bytes.Length)
    $lock.Flush()

    $env:WGPU_DX11_REQUIRE_HARDWARE = '1'
    Remove-Item Env:WGPU_DX11_FORCE_FL9_1 -ErrorAction SilentlyContinue
    $env:WGPU_DX11_EXACT_FEATURE_LEVEL = '11_0'
    Run-Tests 'hal-fl11_0' $hal 'tier_fl11_0'
    Run-Tests 'hal-selector' $hal 'exact_selector'
    Run-Tests 'public-fl11_0' $public
    $env:WGPU_DX11_EXACT_FEATURE_LEVEL = '10_1'
    Run-Tests 'public-fl10_1' $public
    $env:WGPU_DX11_EXACT_FEATURE_LEVEL = '9_1'
    Run-Tests 'public-fl9_1' $public
    $env:WGPU_DX11_EXACT_FEATURE_LEVEL = '12_0'
    Run-Tests 'public-invalid' $public
    if ($IncludeBenchmarks) {
        $env:WGPU_DX11_SCENE_BENCH = '1'
        foreach ($level in '9_1', '10_1', '11_0') {
            $env:WGPU_DX11_EXACT_FEATURE_LEVEL = $level
            Run-Tests "scene-$level" $scene
        }
        Remove-Item Env:WGPU_DX11_SCENE_BENCH
    }
    if ($IncludeFl91Public) {
        Remove-Item Env:WGPU_DX11_EXACT_FEATURE_LEVEL
        $env:WGPU_DX11_FORCE_FL9_1 = '1'
        Run-Tests 'fl91-public-all' $fl91
    }
} finally {
    if ($lock) {
        $lock.Dispose()
        [IO.File]::Delete($lockPath)
    }
    @{
        source_sha = $revision
        source_changes = $dirty
        architecture = $Architecture
        host = $env:COMPUTERNAME
        os = (Get-CimInstance Win32_OperatingSystem).Caption
        os_build = (Get-CimInstance Win32_OperatingSystem).BuildNumber
        driver_packages = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
        d3dcompiler_version = (Get-Item (Join-Path $env:windir ($(if ($Architecture -eq 'x86') { 'SysWOW64' } else { 'System32' }) + '/d3dcompiler_47.dll'))).VersionInfo.FileVersion
        utc = [DateTime]::UtcNow.ToString('o')
        worktree = $repo
        target_directory = $env:CARGO_TARGET_DIR
        rustc = $toolchain
        builds = $builds
        results = $results
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'results.json') -Encoding utf8
    Pop-Location
}
if ($results.Count -eq 0 -or @($results | Where-Object {
    $_.exit_code -ne 0 -or $_.passed -eq 0 -or $_.skipped -ne 0 -or $_.ignored -ne 0
}).Count -gt 0) {
    throw "A suite failed or lacked execution coverage. See $out."
}

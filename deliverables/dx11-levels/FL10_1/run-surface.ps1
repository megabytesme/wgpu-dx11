[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$PortableExecutable,
    [Parameter(Mandatory)][string]$NativeExecutable,
    [Parameter(Mandatory)][string]$Log,
    [string]$CoordinationPath = 'C:/dev/personal/HorizonCodexCoord-Nov2026',
    [string]$Endpoint = 'ben@192.168.1.227',
    [string]$Key = "$env:USERPROFILE/OneDrive/Documents/SSH/Ben-Tablet"
)

$ErrorActionPreference = 'Stop'
$portable = (Resolve-Path -LiteralPath $PortableExecutable).Path
$native = (Resolve-Path -LiteralPath $NativeExecutable).Path
$logPath = [IO.Path]::GetFullPath($Log)
if (Test-Path -LiteralPath $logPath) {
    throw "Choose a new log path: $logPath"
}
$portableHash = (Get-FileHash -LiteralPath $portable).Hash
$nativeHash = (Get-FileHash -LiteralPath $native).Hash
$session = 'wgpu-fl101-' + [Guid]::NewGuid().ToString('N')
$windowsSsh = Join-Path $env:WINDIR 'System32/OpenSSH'
$sshOptions = @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', '-i', $Key)

function Invoke-Tablet([string]$Script) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Script))
    & "$windowsSsh/ssh.exe" @sshOptions $Endpoint powershell.exe -NoProfile -EncodedCommand $encoded
}

$lockPath = Join-Path $CoordinationPath 'locks/surface.lock'
$lock = [IO.File]::Open($lockPath, 'CreateNew', 'Write', 'Read')
try {
    $record = [Text.Encoding]::UTF8.GetBytes((@{
        owner = 'fl10_1'
        host = $env:COMPUTERNAME
        pid = $PID
        utc = [DateTime]::UtcNow.ToString('o')
        action = 'ARM64 Win32 exact FL10.1 native and portable readback'
    } | ConvertTo-Json -Compress))
    $lock.Write($record)
    $lock.Flush()

    $remoteDirectory = Invoke-Tablet ('New-Item -ItemType Directory -Path (Join-Path $env:LOCALAPPDATA "Temp/{0}") -ErrorAction Stop | Select-Object -ExpandProperty FullName' -f $session)
    if ($LASTEXITCODE -ne 0) { throw 'Surface could not create the isolated test directory.' }
    "ARM64 source portable=$portable SHA256=$portableHash" | Set-Content -LiteralPath $logPath
    "ARM64 source native=$native SHA256=$nativeHash" | Add-Content -LiteralPath $logPath
    "ARM64 destination host=$Endpoint directory=$remoteDirectory" | Add-Content -LiteralPath $logPath
    & "$windowsSsh/scp.exe" @sshOptions $portable "${Endpoint}:AppData/Local/Temp/$session/portable.exe"
    if ($LASTEXITCODE -ne 0) { throw 'ARM64 portable test transfer failed.' }
    & "$windowsSsh/scp.exe" @sshOptions $native "${Endpoint}:AppData/Local/Temp/$session/native.exe"
    if ($LASTEXITCODE -ne 0) { throw 'ARM64 native test transfer failed.' }

    $remoteRun = @'
$ErrorActionPreference = 'Stop'
$directory = Join-Path $env:LOCALAPPDATA 'Temp/__SESSION__'
$portable = Join-Path $directory 'portable.exe'
$native = Join-Path $directory 'native.exe'
if ((Get-FileHash -LiteralPath $portable).Hash -ne '__PORTABLE__' -or
    (Get-FileHash -LiteralPath $native).Hash -ne '__NATIVE__') {
    throw 'Transferred binaries differ from local builds.'
}
"host=$env:COMPUTERNAME os=$([Environment]::OSVersion.Version) architecture=$env:PROCESSOR_ARCHITECTURE"
$env:WGPU_DX11_EXPERIMENTAL_FL10_1 = '1'
$env:WGPU_DX11_FORCE_FL9_1 = '0'
& $portable public_dx11_alpha_to_coverage_requires_opt_in_and_resolves --ignored --nocapture --test-threads=1
$portableStatus = $LASTEXITCODE
& $native 'dx11::exact::tests::formats::' --ignored --nocapture --test-threads=1
$nativeStatus = $LASTEXITCODE
"portable status=$portableStatus native status=$nativeStatus"
if ($portableStatus -ne 0 -or $nativeStatus -ne 0) { exit 1 }
'@
    $remoteRun = $remoteRun.Replace('__SESSION__', $session).Replace('__PORTABLE__', $portableHash).Replace('__NATIVE__', $nativeHash)
    Invoke-Tablet $remoteRun 2>&1 | Tee-Object -FilePath $logPath -Append
    $exitCode = $LASTEXITCODE
} finally {
    $lock.Dispose()
    Remove-Item -LiteralPath $lockPath
}
exit $exitCode

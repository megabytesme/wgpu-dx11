[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Package,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$Sha256
)
$ErrorActionPreference = 'Stop'
if ((Get-FileHash -LiteralPath $Package -Algorithm SHA256).Hash -ne $Sha256) {
    throw 'Transferred package does not match the signed build.'
}
Add-AppxPackage -Path $Package
$app = Get-AppxPackage -Name 'WgpuDx11.TierProbe'
if ($app.Architecture -ne 'Arm64' -or $app.Status -ne 'Ok') {
    throw 'Probe is missing or is not registered as ARM64.'
}
$stamp = [DateTime]::UtcNow
Add-Type @'
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IRuntimeActivation {
    void ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string app,
                             [MarshalAs(UnmanagedType.LPWStr)] string args, uint options, out int pid);
}

public static class Dx11ProbeActivation {
    public static int Start(string id) {
        var manager = (IRuntimeActivation)Activator.CreateInstance(Type.GetTypeFromCLSID(
            new Guid("45ba127d-10a8-46ea-8ab7-56ea9078943c"), true));
        try { int pid; manager.ActivateApplication(id, "", 0, out pid); return pid; }
        finally { Marshal.FinalReleaseComObject(manager); }
    }
}
'@
$probePid = [Dx11ProbeActivation]::Start("$($app.PackageFamilyName)!App")
$out = Split-Path -Parent $Package
"$($app.PackageFullName) status=$($app.Status) sha256=$Sha256 pid=$probePid utc=$($stamp.ToString('o'))" |
    Set-Content -LiteralPath (Join-Path $out 'surface-deployment.txt') -Encoding UTF8
$log = Join-Path $env:LOCALAPPDATA "Packages\$($app.PackageFamilyName)\LocalState\dx11-tier-probe.txt"
$deadline = $stamp.AddSeconds(50)
do {
    $entry = Get-Item -LiteralPath $log -ErrorAction SilentlyContinue
    if ($entry -and $entry.LastWriteTimeUtc -ge $stamp -and -not (Get-Process -Id $probePid -ErrorAction SilentlyContinue)) {
        Copy-Item -LiteralPath $log -Destination (Join-Path $out 'surface-raw.txt')
        if (-not (Select-String -LiteralPath $log -Pattern '^overall=PASS$' -Quiet)) {
            throw 'GPU probe did not report a successful smoke test.'
        }
        exit 0
    }
    Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $deadline)
throw 'GPU probe did not finish with a fresh LocalState report.'

[CmdletBinding()]
param([string]$Fxc = (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin\10.0.19041.0\x64\fxc.exe'))
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Fxc -PathType Leaf)) { throw "FXC not found: $Fxc" }
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('wgpu-sm41-fxc-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    Write-Output "compiler=$Fxc version=$((Get-Item -LiteralPath $Fxc).VersionInfo.FileVersion)"
    $cases = @(
        @{ Name='sample-sm40'; Source='sample.hlsl'; Profile='ps_4_0'; Expected=0; Instruction='(?m)^\s*sample\s' },
        @{ Name='gather-sm40'; Source='gather.hlsl'; Profile='ps_4_0'; Expected=1; Diagnostic='X4532' },
        @{ Name='gather-sm41'; Source='gather.hlsl'; Profile='ps_4_1'; Expected=0; Instruction='(?m)^\s*gather4\s' }
    )
    foreach ($case in $cases) {
        $out = Join-Path $scratch "$($case.Name).out"
        $err = Join-Path $scratch "$($case.Name).err"
        $object = Join-Path $scratch "$($case.Name).dxbc"
        $assembly = Join-Path $scratch "$($case.Name).asm"
        $source = Join-Path $PSScriptRoot $case.Source
        $args = "/nologo /Ges /T $($case.Profile) /E main /Fo `"$object`" /Fc `"$assembly`" `"$source`""
        $process = Start-Process -FilePath $Fxc -ArgumentList $args -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden -Wait -PassThru
        $message = (Get-Content -LiteralPath $out, $err -Raw | Out-String).Trim()
        Write-Output "case=$($case.Name) target=$($case.Profile) exit=$($process.ExitCode)"
        if ($message) { Write-Output $message }
        if ($case.Expected -eq 0) {
            if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $object) -or -not (Test-Path -LiteralPath $assembly)) { throw "Compilation failed: $($case.Name)" }
            $text = Get-Content -LiteralPath $assembly -Raw
            if ($text -notmatch "(?m)^$($case.Profile)\s*$" -or $text -notmatch $case.Instruction) { throw "Expected instruction or profile absent: $($case.Name)" }
            Write-Output "instruction=$($Matches[0].Trim()) object_bytes=$((Get-Item -LiteralPath $object).Length)"
        } else {
            if ($process.ExitCode -eq 0 -or $message -notmatch $case.Diagnostic -or (Test-Path -LiteralPath $object)) { throw "Expected profile rejection absent: $($case.Name)" }
            Write-Output "expected_diagnostic=$($case.Diagnostic)"
        }
    }
    Write-Output 'PASS: FXC profile controls'
} finally {
    foreach ($name in 'sample-sm40','gather-sm40','gather-sm41') {
        foreach ($extension in 'out','err','dxbc','asm') {
            $path = Join-Path $scratch "$name.$extension"
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
        }
    }
    Remove-Item -LiteralPath $scratch
}

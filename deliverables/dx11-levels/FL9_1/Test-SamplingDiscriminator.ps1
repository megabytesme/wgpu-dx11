[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$FxcPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-NearestMip([double]$Left, [double]$Right) {
    $footprint = 16.0 * [Math]::Abs($Right - $Left)
    if ($footprint -le 1.0) { return 0 }
    return [Math]::Min(4, [Math]::Floor([Math]::Log($footprint, 2.0) + 0.5))
}

function Get-RepeatFold([double]$U) { return $U - [Math]::Floor($U) }

function Get-MirrorFold([double]$U) { return 1.0 - [Math]::Abs(2.0 * (Get-RepeatFold (0.5 * $U)) - 1.0) }

function Get-EdgeWeights([double]$U, [bool]$Wrap) {
    $position = 16.0 * $U - 0.5
    $left = [int][Math]::Floor($position)
    $rightWeight = $position - $left
    $taps = @($left, ($left + 1)) | ForEach-Object {
        if ($Wrap) { (($_ % 16) + 16) % 16 } else { [Math]::Max(0, [Math]::Min(15, $_)) }
    }
    $red = 0.0
    $blue = 0.0
    $weights = @((1.0 - $rightWeight), $rightWeight)
    for ($index = 0; $index -lt $taps.Count; $index++) {
        if ($taps[$index] -eq 0) { $red += $weights[$index] }
        if ($taps[$index] -eq 15) { $blue += $weights[$index] }
    }
    return @($red, $blue)
}

$repeatLeft = 63.0 / 64.0
$repeatRight = 65.0 / 64.0
$mirrorLeft = 15.0 / 16.0
$mirrorRight = 17.0 / 16.0
$oracle = [ordered]@{
    repeat_edge_native_rb_weights = @(Get-EdgeWeights 1.0 $true)
    repeat_edge_fold_clamp_rb_weights = @(Get-EdgeWeights (Get-RepeatFold 1.0) $false)
    repeat_quad_native_mip = Get-NearestMip $repeatLeft $repeatRight
    repeat_quad_folded_mip = Get-NearestMip (Get-RepeatFold $repeatLeft) (Get-RepeatFold $repeatRight)
    mirror_quad_native_mip = Get-NearestMip $mirrorLeft $mirrorRight
    mirror_quad_folded_mip = Get-NearestMip (Get-MirrorFold $mirrorLeft) (Get-MirrorFold $mirrorRight)
}
if ($oracle.repeat_edge_native_rb_weights[0] -ne 0.5 -or $oracle.repeat_edge_native_rb_weights[1] -ne 0.5 -or
    $oracle.repeat_edge_fold_clamp_rb_weights[0] -ne 1.0 -or $oracle.repeat_edge_fold_clamp_rb_weights[1] -ne 0.0 -or
    $oracle.repeat_quad_native_mip -ne 0 -or $oracle.repeat_quad_folded_mip -ne 4 -or
    $oracle.mirror_quad_native_mip -ne 1 -or $oracle.mirror_quad_folded_mip -ne 0) {
    throw 'Unexpected analytical reference result.'
}

$fxc = Get-Item -LiteralPath $FxcPath
$source = Get-Item -LiteralPath (Join-Path $PSScriptRoot 'SamplingDiscriminator.hlsl')
$entries = [ordered]@{
    native_sample = $true
    uniform_rebase = $true
    repeat_folded = $true
    mirror_folded = $true
    explicit_derivatives = $false
    gradients_after_repeat_fold = $false
    explicit_gradients = $false
    explicit_lod = $false
    explicit_texel = $false
}
$temp = Join-Path ([IO.Path]::GetTempPath()) ('fl91-sampling-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temp | Out-Null

try {
    $checks = foreach ($profile in 'ps_4_0_level_9_1', 'ps_5_0') {
        foreach ($entry in $entries.Keys) {
            $outputPath = Join-Path $temp "$profile-$entry.cso"
            $assemblyPath = Join-Path $temp "$profile-$entry.asm"
            $expected = $profile -eq 'ps_5_0' -or $entries[$entry]
            $PSNativeCommandUseErrorActionPreference = $false
            $ErrorActionPreference = 'Continue'
            $compilerOutput = & $fxc.FullName /nologo /Ges /WX /T $profile /E $entry /Fo $outputPath /Fc $assemblyPath $source.FullName 2>&1
            $exitCode = $LASTEXITCODE
            $ErrorActionPreference = 'Stop'
            $diagnostic = (($compilerOutput | ForEach-Object {
                if ($_ -is [string]) { $_ } else { $_.Exception.Message }
            }) -join [Environment]::NewLine).Replace($source.FullName, $source.Name).Replace($temp, '<temp>')
            $compiled = $exitCode -eq 0 -and (Test-Path -LiteralPath $outputPath -PathType Leaf)
            [pscustomobject]@{
                profile = $profile
                entry = $entry
                expected_compile = $expected
                compiled = $compiled
                passed = ($expected -and $compiled) -or (-not $expected -and $exitCode -ne 0 -and $diagnostic -match 'X4532|X5190')
                compiler_exit_code = $exitCode
                diagnostic = $diagnostic
                legacy_instruction_count = if ($compiled -and $profile -eq 'ps_4_0_level_9_1') {
                    Select-String -LiteralPath $assemblyPath -Pattern '^// approximately .*instruction slots used' |
                        Select-Object -First 1 -ExpandProperty Line
                } else { $null }
            }
        }
    }
    [pscustomobject]@{
        gpu_executed = $false
        compiler_path = $fxc.FullName
        compiler_version = $fxc.VersionInfo.FileVersion
        compiler_sha256 = (Get-FileHash -LiteralPath $fxc.FullName).Hash
        shader_sha256 = (Get-FileHash -LiteralPath $source.FullName).Hash
        analytical_reference = $oracle
        checks = $checks
    } | ConvertTo-Json -Depth 5
    if ($checks.passed -contains $false) { exit 1 }
    exit 0
}
finally {
    Get-ChildItem -LiteralPath $temp -File | ForEach-Object { Remove-Item -LiteralPath $_.FullName }
    Remove-Item -LiteralPath $temp
}

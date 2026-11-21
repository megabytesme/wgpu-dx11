[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$FxcPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function New-ArithmeticShader {
    param([string]$OutputSemantic, [int]$Iterations, [string]$Result)

    @("float4 main(float4 v : TEXCOORD0, float4 addend : TEXCOORD1) : ${OutputSemantic} {") +
        @('v = frac(v + addend);') * $Iterations +
        @("return $Result;", '}')
}

function New-DependentSampleShader {
    param([int]$DependentReads)

    @(
        'Texture2D<float4> image : register(t0);'
        'SamplerState samp : register(s0);'
        'float4 main(float2 uv : TEXCOORD0) : SV_TARGET {'
        'float4 color = image.Sample(samp, uv);'
    ) + @('color = image.Sample(samp, color.xy);') * $DependentReads + @('return color;', '}')
}

function New-VertexLoopShader {
    param([string]$Body)

    @(
        'float4 main(float4 v : TEXCOORD0, float4 addend : TEXCOORD1) : SV_POSITION {'
        "[loop] for(int i=0; i<6; i++){ $Body }"
        'return v;'
        '}'
    )
}

function New-ClipPlaneShader {
    param([int]$PlaneCount)

    $planes = (0..($PlaneCount - 1) | ForEach-Object { "planes[$_]" }) -join ', '
    @(
        'cbuffer Clip { float4 planes[7]; };'
        "[clipplanes($planes)] float4 main(float4 position : POSITION) : SV_POSITION {return position;}"
    )
}

function New-PixelVaryingShader {
    param([int]$Varyings, [ValidateSet('Absent', 'Unused', 'Used')][string]$Position)
    $indices = 0..($Varyings - 1)
    $inputs = @($indices | ForEach-Object { "float4 v$_ : TEXCOORD$_" })
    $terms = @($indices | ForEach-Object { "v$_" })
    if ($Position -ne 'Absent') {
        $inputs += 'float4 position : SV_Position'
    }
    if ($Position -eq 'Used') {
        $terms += 'position'
    }
    "float4 main($($inputs -join ', ')) : SV_TARGET { return $($terms -join ' + '); }"
}

$cases = @(
    @{
        id = 'vertex-instructions-256'
        profile = 'vs_4_0_level_9_1'
        source = New-ArithmeticShader 'SV_POSITION' 126 'v * addend'
        assembly_match = '// approximately 256 instruction slots used'
    }
    @{
        id = 'vertex-instructions-257'
        profile = 'vs_4_0_level_9_1'
        source = New-ArithmeticShader 'SV_POSITION' 127 'v'
        compiler_error = 'X5615:.*\(257\).*256'
    }
    @{
        id = 'pixel-arithmetic-64'
        profile = 'ps_4_0_level_9_1'
        source = New-ArithmeticShader 'SV_TARGET' 31 'v'
        assembly_match = '// approximately 64 instruction slots used'
    }
    @{
        id = 'pixel-arithmetic-65'
        profile = 'ps_4_0_level_9_1'
        source = New-ArithmeticShader 'SV_TARGET' 31 'v * addend'
        compiler_error = 'X5608:.*\(65\).*64'
    }
    @{
        id = 'pixel-dependency-order-three'
        profile = 'ps_4_0_level_9_1'
        source = New-DependentSampleShader 3
        assembly_match = '// approximately 5 instruction slots used \(4 texture, 1 arithmetic\)'
    }
    @{
        id = 'pixel-dependency-order-four'
        profile = 'ps_4_0_level_9_1'
        source = New-DependentSampleShader 4
        compiler_error = 'X5426:'
    }
    @{
        id = 'vertex-static-loop-one'
        profile = 'vs_4_0_level_9_1'
        source = New-VertexLoopShader 'v = frac(v + addend);'
        assembly_match = '(?m)^\s+rep i\d+\b'
    }
    @{
        id = 'vertex-static-loop-two'
        profile = 'vs_4_0_level_9_1'
        source = New-VertexLoopShader '[loop] for(int j=0; j<6; j++){ v = frac(v + addend); }'
        compiler_error = 'X3531:'
    }
    @{
        id = 'vertex-user-clip-planes-six'
        profile = 'vs_4_0_level_9_1'
        source = New-ClipPlaneShader 6
        assembly_match = '(?m)^// clip0\s+cb0\s+0\s+6\s+'
    }
    @{
        id = 'vertex-user-clip-planes-seven'
        profile = 'vs_4_0_level_9_1'
        source = New-ClipPlaneShader 7
        compiler_error = 'X3554:.*6 parameter\(s\) expected, found 7'
    }
    @{
        id = 'pixel-eight-varyings-no-position'
        profile = 'ps_4_0_level_9_1'
        source = New-PixelVaryingShader 8 Absent
        assembly_match = '(?m)^// TEXCOORD\s+7\s+xyzw\s+7\s+'
    }
    @{
        id = 'pixel-seven-varyings-unused-position'
        profile = 'ps_4_0_level_9_1'
        source = New-PixelVaryingShader 7 Unused
        assembly_match = '(?m)^// SV_Position\s+0\s+xyzw\s+7\s+'
    }
    @{
        id = 'pixel-eight-varyings-unused-position'
        profile = 'ps_4_0_level_9_1'
        source = New-PixelVaryingShader 8 Unused
        compiler_error = 'X4506:'
    }
    @{
        id = 'pixel-seven-varyings-read-position'
        profile = 'ps_4_0_level_9_1'
        source = New-PixelVaryingShader 7 Used
        compiler_error = 'X4502:'
    }
)

$fxc = Get-Item -LiteralPath $FxcPath
$temp = Join-Path ([IO.Path]::GetTempPath()) ("fl91-fxc-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temp | Out-Null

try {
    $checks = foreach ($case in $cases) {
        $sourcePath = Join-Path $temp ($case.id + '.hlsl')
        $assemblyPath = Join-Path $temp ($case.id + '.asm')
        $objectPath = Join-Path $temp ($case.id + '.cso')
        [IO.File]::WriteAllLines($sourcePath, [string[]]$case.source, [Text.Encoding]::ASCII)

        $PSNativeCommandUseErrorActionPreference = $false
        $ErrorActionPreference = 'Continue'
        $output = & $fxc.FullName /nologo /Ges /WX /T $case.profile /E main /Fc $assemblyPath /Fo $objectPath $sourcePath 2>&1
        $exitCode = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        $diagnostic = (($output | ForEach-Object ToString) -join [Environment]::NewLine).Replace($temp, '<temp>')
        $assembly = if ([IO.File]::Exists($assemblyPath)) {
            [IO.File]::ReadAllText($assemblyPath)
        } else {
            ''
        }

        if ($case.ContainsKey('compiler_error')) {
            $expected = $case.compiler_error
            $passed = $exitCode -ne 0 -and $diagnostic -match $expected
        } else {
            $expected = $case.assembly_match
            $passed = $exitCode -eq 0 -and $assembly -cmatch $expected
        }

        [pscustomobject]@{
            id = $case.id
            profile = $case.profile
            outcome = if ($passed) { 'PASS' } else { 'FAIL' }
            compiler_exit_code = $exitCode
            expected_pattern = $expected
            observed_assembly = @(
                $assembly -split '\r?\n' |
                    Where-Object { $_ -cmatch '^// (approximately |clip0\s|TEXCOORD\s|SV_Position\s)|^\s+(rep |endrep)' } |
                    ForEach-Object Trim
            )
            diagnostic = $diagnostic
        }
    }

    [pscustomobject]@{
        evidence_type = 'FXC compilation and legacy instruction disassembly'
        gpu_executed = $false
        compiler_path = $fxc.FullName
        compiler_version = $fxc.VersionInfo.FileVersion
        checks = $checks
    } | ConvertTo-Json -Depth 5

    if ($checks.outcome -contains 'FAIL') {
        exit 1
    }
    exit 0
}
finally {
    Get-ChildItem -LiteralPath $temp -File | ForEach-Object { Remove-Item -LiteralPath $_.FullName }
    Remove-Item -LiteralPath $temp
}

[CmdletBinding()]
param(
    [string]$Roadmap = 'C:/dev/personal/wgpu-dx11-roadmap-Nov21',
    [string]$RoadmapRevision = '126114a9fa069194e2bd63ac8e8d46c327a35d83',
    [string]$SdkInclude = 'C:/Program Files (x86)/Windows Kits/10/Include/10.0.19041.0/um'
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$manifest = Join-Path $PSScriptRoot 'tests.json'
$evidence = @{}
if (Test-Path -LiteralPath $manifest) {
    foreach ($test in (Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json).cases) {
        foreach ($id in $test.requirements) {
            if (-not $evidence.ContainsKey($id)) {
                $evidence[$id] = [Collections.Generic.List[object]]::new()
            }
            foreach ($run in $test.runs) {
                if ($run.result -eq 'pass') { $evidence[$id].Add($run) }
            }
        }
    }
}

function Read-RoadmapCsv([string]$Name) {
    $contents = & git -C $Roadmap show "${RoadmapRevision}:deliverables/dx11-levels/$Name"
    if ($LASTEXITCODE -ne 0) { throw "Could not read $Name at $RoadmapRevision" }
    $contents | ConvertFrom-Csv
}

function New-Requirement(
    [string]$Id, [string]$Tier, [string]$Layer, [string]$Requirement,
    [string]$Classification, [string]$Expected, [string]$Source,
    [string]$Owner, [string]$Hal, [string]$Public, [string]$Plan
) {
    $runs = @($evidence[$Id] | Where-Object { $_ })
    $result = 'unknown'
    if ($runs.Count -gt 0) { $result = 'partial' }
    [pscustomobject][ordered]@{
        id = $Id
        tier = $Tier
        layer = $Layer
        requirement = $Requirement
        classification = $Classification
        expected = $Expected
        source = $Source
        code_owner = $Owner
        hal_entry = $Hal
        public_contract = $Public
        test_plan = $Plan
        qualification = $result
        observed_devices = ($runs.device | Sort-Object -Unique) -join '|'
        candidate_sha = ($runs.source_sha | Sort-Object -Unique) -join '|'
        test_command = ($runs.command | Sort-Object -Unique) -join '|'
        log_sha256 = ($runs.log_sha256 | Sort-Object -Unique) -join '|'
    }
}

$levels = '9_1', '9_2', '9_3', '10_0', '10_1', '11_0'
$rows = @(
    foreach ($row in Read-RoadmapCsv 'native-guarantees.csv') {
        if ($row.tier -notin $levels) { continue }
        New-Requirement $row.id $row.tier 'native operation' $row.requirement $row.classification (
            "$($row.native_value) $($row.qualifiers)".Trim()
        ) $row.source $row.owner $row.hal_entry $row.public_mapping $row.test_id
    }
    foreach ($row in Read-RoadmapCsv 'dxgi-format-support.csv') {
        if ($row.tier -ne '11_0' -and ($row.tier -notin $levels -or $row.source_scope -ne 'DDI')) {
            continue
        }
        New-Requirement $row.id $row.tier "$($row.source_scope) format use" "$($row.format).$($row.use)" $row.status (
            $row.condition
        ) $row.source $row.owner $row.hal_entry $row.public_mapping $row.test
    }
    foreach ($header in 'd3d11', 'd3d11_1', 'd3d11_2', 'd3d11_3', 'd3d11_4') {
        $source = Get-Content -LiteralPath (Join-Path $SdkInclude "$header.h") -Raw
        $interfaces = [regex]::Matches($source, '(?s)\b(ID3D11\w+) : public \w+\s*\{\s*public:(.*?)\r?\n\s*\};')
        foreach ($interface in $interfaces) {
            $type = $interface.Groups[1].Value
            $methods = [regex]::Matches($interface.Groups[2].Value, 'STDMETHODCALLTYPE\s+(\w+)\s*\(')
            foreach ($method in $methods) {
                $name = $method.Groups[1].Value
                $id = "FL11_0.METHOD.$type.$name"
                $classification = if ($header -eq 'd3d11') { 'base API, per-operation review' } else { 'runtime or device conditional' }
                $owner = if ($type -match 'DeviceContext') { 'command recording' } else { 'native API extension' }
                New-Requirement $id '11_0' 'native method' "${type}::$name" $classification (
                    'Verify feature-level, interface version, argument limits, completion and error behavior'
                ) "https://learn.microsoft.com/en-us/windows/win32/api/$header/nn-$header-$($type.ToLowerInvariant())" (
                    $owner
                ) 'wgpu-hal/src/dx11' 'See corresponding native operation rows' 'METHOD_API'
            }
        }
    }
    $hal = Get-Content -LiteralPath (Join-Path $repo 'wgpu-hal/src/lib.rs') -Raw
    $interfaces = [regex]::Matches($hal, '(?ms)^pub trait (Instance|Surface|Adapter|Device|Queue|CommandEncoder):[^\r\n]*\{(.*?)^\}')
    foreach ($interface in $interfaces) {
        $type = $interface.Groups[1].Value
        $methods = [regex]::Matches($interface.Groups[2].Value, '(?m)^    (?:unsafe )?fn (\w+)')
        foreach ($method in $methods) {
            $name = $method.Groups[1].Value
            New-Requirement "FL11_0.HAL.$type.$name" '11_0' 'HAL method' "${type}::$name" 'per-operation review' (
                'Exercise corresponding portable operation where supported by the targeted contract'
            ) 'wgpu-hal/src/lib.rs' 'DX11 HAL' 'wgpu-hal/src/dx11' 'wgpu and WebGPU' 'HAL_API'
        }
    }
    $limits = Get-Content -LiteralPath (Join-Path $repo 'wgpu-types/src/limits.rs') -Raw
    $definition = [regex]::Match($limits, '(?ms)^pub struct Limits \{(.*?)^\}').Groups[1].Value
    foreach ($field in [regex]::Matches($definition, '(?m)^    pub (\w+):')) {
        $name = $field.Groups[1].Value
        New-Requirement "FL11_0.WGPU.LIMIT.$name" '11_0' 'public limit' "Limits::$name" 'per-limit review' (
            'Derive from proven backend behavior and validate the exact boundary where applicable'
        ) 'wgpu-types/src/limits.rs' 'selection and projection' 'capabilities.rs' 'wgpu::Limits' 'PUBLIC_LIMIT'
    }
    $features = Get-Content -LiteralPath (Join-Path $repo 'wgpu-types/src/features.rs') -Raw
    $featureSets = [regex]::Matches($features, '(?ms)^    pub struct Features(?:WGPU|WebGPU) \w+ \{(.*?)^    \}')
    foreach ($featureSet in $featureSets) {
        foreach ($feature in [regex]::Matches($featureSet.Groups[1].Value, '(?m)^        const ([A-Z0-9_]+) =')) {
            $name = $feature.Groups[1].Value
            New-Requirement "FL11_0.WGPU.FEATURE.$name" '11_0' 'public feature' "Features::$name" 'per-feature review' (
                'Prove support before advertisement or record absence and rejection'
            ) 'wgpu-types/src/features.rs' 'selection and projection' 'capabilities.rs' 'wgpu::Features' 'PUBLIC_FEATURE'
        }
    }
)
if ($rows.Count -lt 6000 -or ($rows.id | Sort-Object -Unique).Count -ne $rows.Count) {
    throw 'Incomplete or duplicated requirement inventory.'
}
$rows | Export-Csv -LiteralPath (Join-Path $PSScriptRoot 'requirements.csv') -NoTypeInformation -Encoding utf8

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string[]] $ProbeLog,

    [Parameter(Mandatory)]
    [string] $D3D11Header,

    [string] $Contract = (Join-Path $PSScriptRoot 'contract.csv')
)

$ErrorActionPreference = 'Stop'
$flags = @{}
$header = Get-Content -LiteralPath $D3D11Header -Raw
foreach ($entry in [regex]::Matches($header, 'D3D11_FORMAT_SUPPORT_(\w+)\s*=\s*(0x[\da-f]+|\d+)\b', 'IgnoreCase')) {
    $number = $entry.Groups[2].Value
    $flags[$entry.Groups[1].Value] = if ($number.StartsWith('0x')) {
        [Convert]::ToUInt32($number.Substring(2), 16)
    } else {
        [uint32] $number
    }
}
if ($flags.Count -eq 0) {
    throw "No D3D11_FORMAT_SUPPORT flags found in $D3D11Header"
}

$rows = Import-Csv -LiteralPath $Contract | Where-Object id -Like 'DXGI-*'
if (!$rows) {
    throw "No DXGI rows found in $Contract"
}
foreach ($path in $ProbeLog) {
    $adapter = $null
    $exact = $false
    $queries = @{}
    $consumed = 0
    $lines = @(Get-Content -LiteralPath $path) + 'ADAPTER end'
    foreach ($line in $lines) {
        if ($line -match '^ADAPTER (.+)$') {
            $nextAdapter = $Matches[1]
            if ($adapter -and $exact) {
                ++$consumed
                foreach ($row in $rows) {
                    if ($row.id -notmatch '^DXGI-([A-Z0-9_]+)-([A-Z0-9_]+)') {
                        throw "Malformed DXGI row $($row.id)"
                    }
                    $format = $Matches[1]
                    $flag = $Matches[2]
                    $sample = $queries[$format]
                    $result = if (!$flags.ContainsKey($flag)) {
                        'unmapped_sdk_flag'
                    } elseif (!$sample) {
                        'unqueried'
                    } elseif ($sample.hr -ne '00000000') {
                        'query_error'
                    } elseif (($sample.bits -band $flags[$flag]) -eq 0) {
                        'absent'
                    } else {
                        'present'
                    }
                    [pscustomobject]@{
                        log = [IO.Path]::GetFileName($path)
                        adapter = $adapter
                        id = $row.id
                        classification = $row.classification
                        query_result = $result
                        hresult = $sample.hr
                        format_flags = $sample.hex
                    }
                }
            }
            $adapter = $nextAdapter
            $exact = $false
            $queries = @{}
        } elseif ($line -match '^DEVICE hr=00000000 requested=9100 selected=9100 actual=9100 PASS$') {
            $exact = $true
        } elseif ($line -match '^FORMAT (\w+) hr=([\da-f]{8}) flags=([\da-f]{8}) ') {
            $queries[$Matches[1]] = @{
                hr = $Matches[2]
                hex = $Matches[3]
                bits = [Convert]::ToUInt32($Matches[3], 16)
            }
        }
    }
    if ($consumed -eq 0) {
        throw "No exact physical FL9_1 adapter found in $path"
    }
}

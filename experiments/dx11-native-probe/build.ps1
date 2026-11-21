[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('x64','x86','arm','arm64')][string]$Architecture,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [switch]$AppContainer,
    [string]$Publisher,
    [ValidatePattern('^[a-fA-F0-9]{40}$')][string]$SigningThumbprint,
    [string]$SdkRoot = (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'),
    [string]$MsvcToolset = (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207')
)

$ErrorActionPreference = 'Stop'
$uwp = $AppContainer.IsPresent
if (-not $uwp -and ($Publisher -or $SigningThumbprint)) {
    throw 'Use -AppContainer when specifying an AppX publisher or development signer.'
}
if ($uwp -and $Publisher -and $SigningThumbprint) {
    throw 'Choose either an unsigned package publisher or a development signer.'
}
if ($uwp -and -not ($Publisher -or $SigningThumbprint)) {
    throw 'Supply -Publisher for an unsigned package or -SigningThumbprint for a signed package.'
}
if ($SigningThumbprint) {
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$SigningThumbprint"
    if (-not $certificate.HasPrivateKey) { throw 'The selected development certificate has no private key.' }
    $Publisher = $certificate.Subject
}
$out = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $out) { throw "Choose a fresh output directory: $out" }
New-Item -ItemType Directory -Path $out | Out-Null
$sdkVersion = '10.0.19041.0'
$linkVersion = if ($Architecture -eq 'arm') { '10.0.15063.0' } else { $sdkVersion }
$tools = Join-Path $MsvcToolset "bin\Hostx64\$Architecture"
$sdkBin = Join-Path $SdkRoot "bin\$sdkVersion\x64"
$include = Join-Path $SdkRoot "Include\$sdkVersion"
$lib = Join-Path $SdkRoot "Lib\$linkVersion"
$compile = @('/nologo','/c','/std:c++17','/EHsc','/O2','/MD','/W4',"/I$out",
    "/I$MsvcToolset\include","/I$include\ucrt","/I$include\um","/I$include\shared","/I$include\winrt")
$link = @('/nologo','/Brepro',"/MACHINE:$Architecture","/LIBPATH:$MsvcToolset\lib\$Architecture",
    "/LIBPATH:$lib\ucrt\$Architecture","/LIBPATH:$lib\um\$Architecture")

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$(Split-Path -Leaf $Program) failed with exit code $LASTEXITCODE." }
}

Push-Location -LiteralPath $out
try {
    $shaders = @{
        vs91 = 'vertex vs_4_0_level_9_1'; ps91 = 'pixel ps_4_0_level_9_1'
        vs93 = 'vertex vs_4_0_level_9_3'; ps93 = 'pixel ps_4_0_level_9_3'
        vs40 = 'vertex vs_4_0'; ps40 = 'pixel ps_4_0'; cs40 = 'compute cs_4_0'
        vs41 = 'vertex vs_4_1'; ps41 = 'pixel ps_4_1'; cs41 = 'compute cs_4_1'
        vs50 = 'vertex vs_5_0'; ps50 = 'pixel ps_5_0'; cs50 = 'compute cs_5_0'
    }
    $pixelProfiles = @{
        '91'='ps_4_0_level_9_1'; '93'='ps_4_0_level_9_3';
        '40'='ps_4_0'; '41'='ps_4_1'; '50'='ps_5_0'
    }
    foreach ($suffix in $pixelProfiles.Keys) {
        $shaders["mirror_ps$suffix"] = "sample_mirror_once $($pixelProfiles[$suffix]) reference.hlsl"
        $shaders["format_ps$suffix"] = "sample_format $($pixelProfiles[$suffix]) reference.hlsl"
    }
    $shaders['logic_ps50'] = 'logic_source ps_5_0 reference.hlsl'
    foreach ($model in '4_0','4_1','5_0') {
        $suffix = $model.Replace('_','')
        foreach ($entry in 'stage_vs','stage_vs_clip','stage_ps','stage_ps_clip','stage_gs') {
            $stage = ($entry -split '_')[1]
            $shaders["$entry$suffix"] = "$entry $($stage)_$model stages.hlsl"
        }
    }
    $shaders['stage_hs50'] = 'stage_hs hs_5_0 stages.hlsl'
    $shaders['stage_ds50'] = 'stage_ds ds_5_0 stages.hlsl'
    $header = foreach ($name in ($shaders.Keys | Sort-Object)) {
        $entry, $profile, $source = $shaders[$name] -split ' '
        if (-not $source) { $source = 'shaders.hlsl' }
        $shaderHeader = Join-Path $out "$name.h"
        Invoke-Checked "$sdkBin\fxc.exe" @('/nologo','/Ges','/WX','/E',$entry,'/T',$profile,
            '/Vn',$name,'/Fh',$shaderHeader,"$PSScriptRoot\$source") | Out-Host
        "#include `"$name.h`""
    }
    $header | Set-Content -LiteralPath dxbc.h -Encoding Ascii
    if ($uwp) {
        $compile += @('/DWINAPI_FAMILY=WINAPI_FAMILY_APP','/D_WIN32_WINNT=0x0A00')
    }
    Invoke-Checked "$tools\cl.exe" ($compile + @('/Foprobe.obj',"$PSScriptRoot\probe.cpp"))

    if (-not $uwp) {
        Invoke-Checked "$tools\link.exe" ($link + @('/SUBSYSTEM:CONSOLE','/OUT:Dx11TierProbe.exe',
            'probe.obj','d3d11.lib','dxgi.lib'))
        return
    }

    $manifestPublisher = [Security.SecurityElement]::Escape($Publisher)
    $stage = New-Item -ItemType Directory -Path stage
    $assetPath = New-Item -ItemType Directory -Path (Join-Path $stage.FullName 'Assets')
    Add-Type -AssemblyName System.Drawing
    $logos = @{ 'StoreLogo.png' = 50; 'Square150x150Logo.png' = 150; 'Square44x44Logo.png' = 44 }
    foreach ($logo in $logos.Keys) {
        $bitmap = [Drawing.Bitmap]::new($logos[$logo], $logos[$logo])
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([Drawing.Color]::FromArgb(20, 36, 58))
            } finally {
                $graphics.Dispose()
            }
            $bitmap.Save((Join-Path $assetPath.FullName $logo), [Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $bitmap.Dispose()
        }
    }
    Invoke-Checked "$tools\cl.exe" ($compile + @('/ZW',"/AI$SdkRoot\UnionMetadata\$sdkVersion",
        "/AI$MsvcToolset\lib\x86\store\references",'/Fouwp.obj',"$PSScriptRoot\uwp.cpp"))
    Invoke-Checked "$tools\link.exe" (@("/LIBPATH:$MsvcToolset\lib\$Architecture\store") + $link +
        @('/SUBSYSTEM:WINDOWS','/APPCONTAINER','/NODEFAULTLIB:kernel32.lib','/INCLUDE:_tls_used',
            '/MAP:probe.map','/OUT:stage\Dx11TierProbe.exe','vccorlib.lib','uwp.obj','probe.obj','WindowsApp.lib',
            'd3d11.lib','dxgi.lib'))
    Invoke-Checked "$tools\dumpbin.exe" @('/nologo','/headers','/imports','stage\Dx11TierProbe.exe') |
        Set-Content -LiteralPath app-pe.txt

    $minimum = if ($Architecture -eq 'arm') { '10.0.15063.0' } else { '10.0.16299.0' }
    @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10" IgnorableNamespaces="uap">
  <Identity Name="WgpuDx11.TierProbe" Publisher="$manifestPublisher" Version="1.0.0.1" ProcessorArchitecture="$Architecture"/>
  <Properties>
    <DisplayName>wgpu D3D11 tier probe</DisplayName>
    <PublisherDisplayName>wgpu D3D11 development</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Universal" MinVersion="$minimum" MaxVersionTested="$sdkVersion"/>
    <PackageDependency Name="Microsoft.VCLibs.140.00" Publisher="CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US" MinVersion="14.0.0.0"/>
  </Dependencies>
  <Resources><Resource Language="en-us"/></Resources>
  <Applications>
    <Application Id="App" Executable="Dx11TierProbe.exe" EntryPoint="Dx11TierProbe.App">
      <uap:VisualElements DisplayName="wgpu D3D11 tier probe" Description="Run native offscreen feature level checks"
        BackgroundColor="#14243A" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png"/>
    </Application>
  </Applications>
</Package>
"@ | Set-Content -LiteralPath stage/AppxManifest.xml -Encoding UTF8
    $package = Join-Path $out "Dx11TierProbe-$Architecture.appx"
    Invoke-Checked "$sdkBin\makeappx.exe" @('pack','/d',$stage.FullName,'/p',$package)
    if ($SigningThumbprint) {
        Invoke-Checked "$sdkBin\signtool.exe" @('sign','/fd','SHA256','/sha1',$SigningThumbprint,$package)
        Invoke-Checked "$sdkBin\signtool.exe" @('verify','/pa',$package)
    }
    Get-FileHash -LiteralPath $package -Algorithm SHA256
} finally {
    Pop-Location
}

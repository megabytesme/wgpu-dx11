$ErrorActionPreference = 'Stop'
& $env:UWP_STRICT_LINK_PS1 -Objects @('App.obj') `
    -RustLibrary $env:DX11_PROBE_LIB -Output (Join-Path (Get-Location).Path 'Dx11SurfaceProbe.exe') `
    -Machine $env:DX11_PROBE_MACHINE -ExtraLibraries @('d3d11.lib', 'dxgi.lib')
if (-not $?) { exit 1 }

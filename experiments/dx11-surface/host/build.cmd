@echo off
setlocal
if not defined DX11_PROBE_LIB exit /b 1
if not defined UWP_STRICT_LINK_PS1 exit /b 1
if not defined WindowsSdkDir exit /b 1
if not defined DX11_PROBE_MACHINE set "DX11_PROBE_MACHINE=X64"
if not exist "%~dp0build" mkdir "%~dp0build"
pushd "%~dp0build"
cl /nologo /c /EHsc /ZW /std:c++17 /O2 /MD /DWINAPI_FAMILY=WINAPI_FAMILY_APP /AI"%WindowsSdkDir%UnionMetadata\10.0.15063.0" "%~dp0App.cpp"
if errorlevel 1 (popd & exit /b 1)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0link.ps1"
set "RESULT=%errorlevel%"
popd
exit /b %RESULT%

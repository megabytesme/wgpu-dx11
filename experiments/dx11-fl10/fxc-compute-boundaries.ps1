$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class FxcControl {
    [DllImport("d3dcompiler_47.dll", CallingConvention = CallingConvention.StdCall)]
    static extern int D3DCompile(byte[] source, UIntPtr size, IntPtr name, IntPtr defines,
        IntPtr include, [MarshalAs(UnmanagedType.LPStr)] string entry,
        [MarshalAs(UnmanagedType.LPStr)] string target, uint flags, uint flags2,
        out IntPtr code, out IntPtr errors);
    [ComImport, Guid("8BA5FB08-5195-40E2-AC58-0D989C3A0102"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface Blob {
        [PreserveSig] IntPtr GetBufferPointer();
        [PreserveSig] UIntPtr GetBufferSize();
    }
    public static int Compile(string source, string target, out int size, out string diagnostics) {
        byte[] bytes = Encoding.UTF8.GetBytes(source);
        IntPtr code, errors;
        int result = D3DCompile(bytes, (UIntPtr)bytes.Length, IntPtr.Zero, IntPtr.Zero,
            IntPtr.Zero, "main", target, 1u << 11, 0, out code, out errors);
        size = 0;
        diagnostics = "";
        try {
            if (code != IntPtr.Zero) {
                var blob = (Blob)Marshal.GetObjectForIUnknown(code);
                size = checked((int)blob.GetBufferSize().ToUInt64());
                Marshal.ReleaseComObject(blob);
            }
            if (errors != IntPtr.Zero) {
                var blob = (Blob)Marshal.GetObjectForIUnknown(errors);
                diagnostics = Marshal.PtrToStringAnsi(blob.GetBufferPointer(), checked((int)blob.GetBufferSize().ToUInt64())).TrimEnd('\0');
                Marshal.ReleaseComObject(blob);
            }
        } finally {
            if (code != IntPtr.Zero) Marshal.Release(code);
            if (errors != IntPtr.Zero) Marshal.Release(errors);
        }
        return result;
    }
}
"@

$structured = @'
RWStructuredBuffer<uint> result : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) { result[id.x] = id.x; }
'@
$typed = @'
RWTexture2D<float4> result : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) { result[id.xy] = float4(1, 2, 3, 4); }
'@
$cases = @(
    @{ Name = 'structured-uav-cs4'; Source = $structured; Target = 'cs_4_0'; Success = $true },
    @{ Name = 'typed-uav-cs4'; Source = $typed; Target = 'cs_4_0'; Success = $false; Error = 'X3666' },
    @{ Name = 'typed-uav-cs5'; Source = $typed; Target = 'cs_5_0'; Success = $true }
)
foreach ($case in $cases) {
    $size = 0
    $diagnostics = ''
    $hr = [FxcControl]::Compile($case.Source, $case.Target, [ref]$size, [ref]$diagnostics)
    $success = $hr -ge 0 -and $size -gt 0
    Write-Host ("{0}: target={1} HRESULT=0x{2:X8} bytes={3} expected={4}" -f $case.Name, $case.Target, ([uint32]([int64]$hr -band 0xffffffffL)), $size, $case.Success)
    if ($diagnostics) { Write-Host $diagnostics }
    if ($success -ne $case.Success -or ($hr -ge 0 -and $size -eq 0)) { throw "Unexpected FXC result: $($case.Name)" }
    if ($case.Error -and $diagnostics -notmatch $case.Error) { throw "Unexpected FXC diagnostic: $($case.Name)" }
}

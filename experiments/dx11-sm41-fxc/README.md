# FXC SM4.1 gather boundary

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File experiments/dx11-sm41-fxc/run.ps1`
on Windows. The optional `-Fxc` parameter chooses a different Microsoft FXC
binary; the default is the Windows 10 SDK 10.0.19041.0 x64 compiler. Scratch
objects and assembly are created in the system temp directory and removed.

The control compiles an ordinary `Texture2D.Sample` as `ps_4_0`, rejects the
same kind of texture's `Gather` with FXC diagnostic X4532 as `ps_4_0`, and
compiles that identical gather shader as `ps_4_1`. It checks the emitted profile
and `sample` or `gather4` assembly instruction, so a successful compile cannot
silently optimize away the operation. Microsoft documents `gather4` as an
[SM4.1 addition](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-sm4-asm)
and lists [`Gather` as supported at SM4.1](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-to-gather).

The [receipt](receipt.txt) records the compiler version and hashes, exact
profiles, rejection and generated instruction on Ben-Desktop. This CPU-only
compiler control does not execute a shader, validate sampling results or device
format support, or change any wgpu capability or default behavior.

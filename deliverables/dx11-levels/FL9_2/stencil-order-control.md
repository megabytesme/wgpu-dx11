# Direct draw stencil ordering control

`stencil_native_probe.cpp` contains an additional native FL9_2 diagnostic. One
direct nonindexed triangle-list draw contains four overlapping triangles in the
sequence front, back, front, back. Front replaces stencil with 1; back increments
it. A stencil-equal draw checks all 256 pixels for value 2. Complementary-cull
replay submits both face states adjacent for each original triangle and checks
the same value. A deliberately regrouped replay submits both front primitives
before both back primitives and checks value 3. The unequal expected values make
loss of primitive order observable. All cases preserve the original vertex offsets.

The direct asymmetric result is diagnostic because this fixture was designed to
investigate a driver that mishandles native two-sided stencil. The two symmetric
replay controls contribute to the existing control pass/fail result. This does
not implement or expose a replay path in wgpu and does not certify public API
behavior. In particular, it does not test instancing: Microsoft documents
[`DrawInstanced` as unsupported on 9.x](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context).

Build from a developer command prompt using MSVC, with scratch paths substituted:

```cmd
cl /nologo /EHsc /W4 /WX /O2 /std:c++17 /Fe:<scratch>\stencil_native_probe.exe /Fo:<scratch>\probe.obj deliverables\dx11-levels\FL9_2\stencil_native_probe.cpp /link d3d11.lib dxgi.lib d3dcompiler.lib
```

The executable accepts an optional DXGI adapter index and requests exact FL9_2.
Acquire the shared device lock before running it. A physical Surface execution
and a public wgpu replay test remain separate work. The [RTX receipt](stencil-order-receipt.txt)
records a successful exact FL9_2 run on Ben-Desktop: the direct and ordered
cases matched value 2 and the regrouped control matched value 3 at all pixels.

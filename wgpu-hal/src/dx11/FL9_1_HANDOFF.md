# FL9_1 handoff

**Outcome: PARTIALLY COMPLETE.** The mission is active. The private D3D11 HAL
can render on a forced-FL9_1 RTX 3090 Ti, but ordinary wgpu adapter enumeration
is disabled, and no public wgpu D3D11 example or HWND presentation has been
verified. A hidden HWND test has exercised the private surface, but visible
presentation and surface lifetime/concurrency still require verification.
A private D24S8 attachment path is implemented and its textured
depth/readback test has passed on forced FL9_1; stencil tests, read-only passes,
aspect-specific views and general render-pass operations remain incomplete.
See FL9_1_PLAN.md for completion gates,
FL9_1_PROGRESS.md for current commands and evidence, and CAPABILITIES.md for the
current implementation and validation matrix.

Resume on `uwp-dx11`; inspect `git status` and recent history before editing.
Private HWND support was checkpointed as `bf56c17b9`. Do not infer full FL9_1 or
historical-device compatibility from modern GPU tests. Keep public adapter
enumeration gated until core-valid operations are safe.

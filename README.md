# MaaWizard

Visual **record → generate pipeline → run on a real device** tooling built on top of
[MaaFramework](https://github.com/MaaXYZ/MaaFramework).

MaaWizard is the "key-mouse macro recorder" that Maa's ecosystem is missing: instead of
replaying raw coordinates, MaaWizard turns what you do into a MaaFramework pipeline that
**recognizes images and text**, so it keeps working when resolution or layout changes.

## Why

Existing tools in the ecosystem target different audiences:

| Tool | Gap |
| --- | --- |
| MaaInspector | Developer-oriented; you must know the Pipeline protocol |
| MaaPipelineEditor | Web editor; needs a local bridge to run |
| General GUIs (MFAAvalonia, etc.) | Only run pre-built resource bundles |
| KeymouseGo | Pure coordinate replay, no image recognition |

MaaWizard closes the gap: **record what you do, get a runnable pipeline immediately.**

## Features

- **Smart recording** — global mouse/keyboard capture via `inputbot`; a click is turned into
  `TemplateMatch + Click` with the template cropped from the screen, not a hard-coded coordinate
- **Text and key awareness** — consecutive keystrokes merge into a single `InputText`,
  function keys become `ClickKey` with the proper virtual key code
- **Visual ROI picking** — drag a rectangle on a live screenshot to crop a template or set a `roi`
- **Full Pipeline editor** — node graph (Vue Flow) with `next` / `on_error` edges and
  `[JumpBack]` markers; every MaaFramework recognition and action type supported
- **Built-in guidance** — each recognition/action ships with effect, use case, parameter docs,
  and contextual warnings (e.g. `Shell` only works on Adb controllers)
- **Schema validation** — catches unknown types, missing required parameters and dangling
  jumps, and points you at the exact node and field
- **V1 / V2 protocol support** — import and export either the flat V1 format or the
  `{type, param}` V2 format; unrecognized fields are preserved round-trip
- **Device management** — ADB devices and Win32 windows
- **Live debug echo** — node execution events stream back to the UI
- **AI ready** — optionally drive `maafw-cli` / `MaaMCP` as external subprocesses

## Quick start

Requirements: Windows (Win32 controller), Rust stable with the MSVC toolchain, Node 18+.

```bash
make deps          # install frontend dependencies
make fetch-sdk     # download the official MaaFramework runtime into ./maa-sdk
make dev           # launch in development mode
```

To produce an installer:

```bash
make build
```

In the app: load `maa-sdk/bin/MaaFramework.dll`, load the `resource` bundle, connect a
controller (Win32 window or ADB device), then run the `Demo` entry node.

## Architecture

```text
Vue 3 + Vue Flow (WebView)
        │  Tauri commands
Rust backend (src-tauri)
  ├─ maa/        M0  runtime wrapper around maa-framework-rs
  ├─ pipeline/   M1  PipelineDocument model (V1/V2) + validation
  ├─ recorder/   M2  input capture → steps → nodes
  ├─ capture/    M3  screenshot, ROI crop, template saving
  ├─ device/     M4  ADB devices, Win32 windows
  └─ ai/         M6  python/uv/uvx detection, external subprocesses
```

Per ADR 0002 the Rust side owns the `PipelineDocument`: the recorder appends nodes to it,
the graph editor renders a snapshot of it, and every edit is committed back through a command.

## Documentation

- `docs/开发规划.md` — roadmap and background (Chinese)
- `docs/spec.md` — functional specification (Chinese)
- `docs/CONTEXT.md` — MaaFramework domain glossary (Chinese)
- `docs/test-plan.md` — testing strategy (Chinese)
- `docs/adr/` — architecture decision records
- `docs/验收清单.md` — 真机手动验收步骤
- `docs/handoff-2026-08-28.md` — session handoff notes

## Development

```bash
make lint     # markdownlint + vue-tsc + cargo clippy
make test     # cargo test + vitest run（全部测试套件需全绿）
make check    # fast cargo check
```

All three linters and both test suites must be green before merging.

## Compliance

MaaFramework is licensed under LGPL-3.0. MaaWizard is intended for **lawful automation and
black-box testing only**. Do not use it to cheat in games or to defeat anti-cheat mechanisms.

# AGENTS.md — MaaWizard

## 定位

基于 MaaFramework 的「可视化录制 + 节点编辑 + 一键真机运行」通用自动化 GUI（"Maa 版按键精灵"）：把用户操作录成基于图像/文字识别的 MaaFramework pipeline，抗分辨率/布局变化，零代码面向大众。

## 怎么跑起来

- 依赖：Windows（Win32 控制器）、Rust stable + MSVC 工具链、Node 18+（本机实测 Node 26）。
- 安装与开发：`make deps` → `make fetch-sdk`（下载运行时到 ./maa-sdk）→ `make dev`。
- 构建安装包：`make build`（= `npm run tauri build`，当前仅产出 NSIS 安装包）。
- 质量门禁：`make lint`（markdownlint + vue-tsc + cargo clippy）、`make test`（cargo test + vitest）。

## 技术栈

Rust + Tauri 2（Windows 原生，非 WSL）+ Vue3 + Vue Flow + 官方绑定 `maa-framework-rs`（feature `dynamic`，运行期 `load_library` 加载预编译库）。

## 目录与约定

- 前端 `src/`：Vue3 + Vue Flow；三标签页「运行 / 图编辑器 / 录制」，设备连接面板已并入运行页。
- 后端 `src-tauri/src/`：`maa/`(M0 运行时)、`pipeline/`(PipelineDocument+V1/V2+校验)、`recorder/`(采集)、`capture/`(截屏/ROI)、`device/`(Win32/ADB)、`ai/`(环境探测)、`commands.rs`(命令层)。
- 资源：`resource/`（pipeline/、image/）；运行期库：`maa-sdk/`（dynamic 链接，编译期不需要）。
- 命令统一走 Makefile；清理动作封装在 `make clean`，禁止直接 `rm`。无 make 时用 PowerShell 等价命令。
- 提交：用 Windows 端 git，非交互；推送身份用一次性 `git -c user.*` 指定，不改全局配置。

## 当前状态与下一步

阶段 0~5 已全实现并构建通过（exe 免安装 + NSIS 安装包）。待办：补齐文档承诺的「导出 interface.json 供 MFAAvalonia 加载」能力（当前代码 0 匹配）；SDK 仍可能需后台下载用于真机运行。

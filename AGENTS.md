# AGENTS.md — MaaWizard

## 定位

基于 MaaFramework 的「可视化录制 + 节点编辑 + 一键真机运行」通用自动化 GUI（"Maa 版按键精灵"）：把用户操作录成基于图像/文字识别的 MaaFramework pipeline，抗分辨率/布局变化，零代码面向大众。

## 怎么跑起来

- 依赖：Windows（Win32 控制器）、Rust stable + MSVC 工具链、Node 18+（本机实测 Node 26）。
- 安装与开发：`make deps` → `make fetch-sdk`（下载运行时到 ./maa-sdk）→ `make dev`。
- 构建安装包：`make build`（= `npm run tauri build`，当前仅产出 NSIS 安装包）。
- 质量门禁：`make lint`（markdownlint + vue-tsc + cargo clippy）、`make test`（cargo test + vitest）。

## 技术栈

Rust + Tauri 2（Windows 原生，非 WSL）+ Vue3 + Vue Flow + 官方绑定 `maa-framework-rs`（feature `dynamic`，运行期 `load_library` 加载预编译库）。图编辑器默认内嵌 MaaPipelineEditor（React，离线静态资源在 public/mpe，经本地 WS 文件桥读写 resource/）。

## 目录与约定

- 前端 `src/`：Vue3 + Vue Flow；三标签页「运行 / 图编辑器 / 录制」，设备连接面板已并入运行页；图编辑器页默认内嵌 MaaPipelineEditor（iframe → /mpe/），自研 Vue Flow 编辑器保留为回退。
- 后端 `src-tauri/src/`：`maa/`(M0 运行时)、`pipeline/`(PipelineDocument+V1/V2+校验)、`recorder/`(采集)、`capture/`(截屏/ROI)、`device/`(Win32/ADB)、`ai/`(环境探测)、`mpe_bridge.rs`(MPE LocalBridge 兼容 WS 文件桥，端口 9066)、`commands.rs`(命令层)。
- 资源：`resource/`（pipeline/、image/）；运行期库：`maa-sdk/`（dynamic 链接，编译期不需要）。
- MPE 源码：`tools/MaaPipelineEditor/` 以 **git subtree** 纳入，来源是 fork `schalkiii/MaaPipelineEditor`（上游 `kqcoxn/MaaPipelineEditor`）的 `maawizard` 分支（= 上游 `main` + 本地 patch），remote 名 `mpe`。同步与打 patch 见「MPE 同步与 patch 管理」。
- MPE 离线资源：`public/mpe/` 是 iframe 内嵌用的构建产物（由 `tools/MaaPipelineEditor/Editor` 的 `mpe` 模式构建，`vite.config.ts` 对 `mode==="mpe"` 设 `base="/mpe/"`），与 subtree 源码同源。**已 gitignore**（衍生产物，约 25 MB / 411 文件，不入库；`public/` 下无其他需跟踪的静态资源）。重建统一走 `make mpe`（等价手工步骤：`cd tools/MaaPipelineEditor/Editor` → `npm install` → `npm run build -- --mode mpe` → `dist/` 拷到 `public/mpe`）。
- 命令统一走 Makefile；清理动作封装在 `make clean`，禁止直接 `rm`。无 make 时用 PowerShell 等价命令。
- 提交：用 Windows 端 git，非交互；推送身份用一次性 `git -c user.*` 指定，不改全局配置。

## MPE 同步与 patch 管理

- 本地 patch 共 3 个文件：`Editor/src/services/server.ts`（`ws://localhost`→`ws://127.0.0.1`，修 Rust 桥只绑 IPv4 而浏览器解析到 `::1`）、`Editor/src/stores/ui/newcomerStore.ts`（新增 `skip()`）、`Editor/src/components/modals/NewcomerGuideModal.tsx`（介绍页与两个答题页加「跳过答题」）。
- 改本地 patch：直接在 `tools/MaaPipelineEditor/` 里改并提交，再 `git subtree push --prefix=tools/MaaPipelineEditor mpe maawizard` 推回 fork。
- 同步上游：在 fork 的 `maawizard` 分支上 merge/rebase 上游新提交（解决冲突后 push），然后在本仓库 `git subtree pull --prefix=tools/MaaPipelineEditor mpe maawizard --squash`。
- 坑：本仓库未设全局 git 身份，subtree 的 merge commit 要内联 `git -c user.name=<u> -c user.email=<u>@users.noreply.github.com`；且 `subtree add/pull` 要求工作区干净——有未提交改动时先 `git stash`，完事 `git stash pop` 还原。

## 当前状态与下一步

阶段 0~5 已全实现并构建通过（exe 免安装 + NSIS 安装包）。待办：补齐文档承诺的「导出 interface.json 供 MFAAvalonia 加载」能力（当前代码 0 匹配）；SDK 仍可能需后台下载用于真机运行。

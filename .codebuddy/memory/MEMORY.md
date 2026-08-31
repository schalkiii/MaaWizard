# 长期记忆 (MEMORY.md)

## 项目：MaaGeneral / MaaWizard

- **定位**：基于 MaaFramework 的「可视化录制 + 节点编辑 + 一键真机运行」通用自动化 GUI，目标为"Maa 版按键精灵"（零代码、大众向、基于图像识别抗变）。
- **现状**：`d:/workspace/MaaGeneral` 为空白工程（仅 .codebuddy），处于规划阶段。
- **技术栈（已定）**：Rust + Tauri 2 + Vue3 + Vue Flow + 官方 Rust 绑定 `maa-framework-rs`（crate `maa-framework`，feature `dynamic`，`load_library` 加载预编译库）。
- **选型依据**：MaaInspector 已用「Tauri(Rust)+Vue3+vue-flow」同栈验证；`maa-framework-rs` 为官方绑定，免去手写 bindgen。
- **规划文档**：`docs/开发规划.md`（v0.1，含阶段0~5路线图）。

## MaaFramework 生态关键事实（可复用）

- **编辑+运行 GUI 现状**：MaaInspector（Tauri，仅 Win，需懂 Pipeline 协议）、MaaPipelineEditor（Web，运行需 LocalBridge）；通用 GUI 运行器（MFAAvalonia/MXU/MFW-CFA/MWU）仅运行预置资源包；按键精灵类（KeymouseGo）纯坐标录制不用 Maa。
- **AI Agent CLI/MCP（对我可用）**：`maafw-cli`（Python CLI，token 省）、`MaaMCP`（MCP Server，run_pipeline 等）、`create-maa-project`（CLI+MCP 脚手架）、`Everything-Maa`（Skills+MCP 配置）、`MaaEvidenceKit`、`maaframework-skills`。
- **空白**：无"大众向、录制即用、基于 Maa 图像识别"的 GUI —— 即本项目要填补的空缺。
- **合规**：仅限合法自动化黑盒测试，禁止外挂/破坏反作弊（LGPL-3.0）。

## 环境与工程状态（2026-08-28）

- **Windows 原生 Rust 已装**（winget Rustlang.Rustup）：rustup 1.29 / rustc 1.98.0 / cargo 1.98.0，`stable-x86_64-pc-windows-msvc`，MSVC 链接器可用。Node v26.7.0 / npm 11.19.0。Rust 亦存在于 WSL（/root/.cargo/bin），但本项目必须用 Windows 原生（Win32 控制器 + Tauri）。
- **工程已搭建**：`d:/workspace/MaaGeneral` = Tauri2 + Vue3 + Vite + Vue Flow + maa-framework(dynamic)。**阶段 0~5 全部实现并构建通过**（`cargo check` 零警告、`npm run build` 38 模块）。
- **后端模块划分**：`maa/`(M0运行时)、`pipeline/`(PipelineDocument+V1/V2转换)、`recorder/`(inputbot采集+智能模板+步骤转节点)、`capture/`(scrap截屏+ROI裁剪)、`device/`(Win32窗口列举)、`ai/`(环境探测+外部子进程)、`commands.rs`(命令层)。
- **前端结构**：`App.vue` 五标签页(运行/图编辑器/录制/设备/AI)；组件 `GraphEditor`(Vue Flow 分层布局，next=蓝实线/on_error=红虚线/[JumpBack]合成节点)、`NodeInspector`(动态表单+帮助)、`RoiCapture`(截图拖拽框选)、`RecorderPanel`、`DevicePanel`、`AiPanel`；`help/registry.ts` 为使用指引注册表。
- **Git 仓库**：`https://github.com/schalkiii/MaaWizard`，main 分支，49 文件 1.4 万行。`.gitignore` 排除 node_modules/dist/target/gen/maa-sdk/.codebuddy。推送身份用 `git -c user.name=<gh用户名> -c user.email=<gh用户名>@users.noreply.github.com` 一次性指定，不改全局 config。
- **质量门禁（固化到 npm scripts / Makefile）**：`make lint` = markdownlint-cli2（`maa-sdk/` 忽略）+ vue-tsc --noEmit + cargo clippy，三者全绿；`make test` = `cargo test`（38）+ `vitest run`（34），合计 72 个测试全绿。
- **前端测试**：vitest 4 + @vue/test-utils 2.5 + jsdom 30；配置 `vitest.config.ts`（jsdom、用例匹配 `src/**/*.spec.ts`），`src/test/setup.ts` 补 ResizeObserver。Vue Flow 相关组件用替身把 nodes/edges 渲染成文本断言，规避 jsdom 无布局能力。
- **Pipeline 校验**：`src-tauri/src/pipeline/validate.rs` 校验未知类型 / 必填参数 / 悬空跳转 / 纯坐标脆弱性，命令 `pipeline_validate`，前端「校验」面板可点击定位到节点；`save()` 顺带提示错误数。
- **MaaFramework SDK**：`maa-sdk/`（MAA-win-x86_64-v5.12.3），经 `make fetch-sdk` 下载。dynamic 链接下编译期不需要 SDK，运行期 `load_library` 才需要。
- **命令入口**：Makefile（make deps/check/dev/build/fetch-sdk/clean/distclean），清理动作封装在 make clean，不直接执行 rm。

## 环境踩坑（复用价值高）

- **Tauri 图标**：256×256 的 PNG 压缩 .ico 会导致 `RC.EXE failed to compile`。必须用 `Bitmap.GetHicon()` 生成标准 BMP 格式图标条目。
- **PowerShell 5.1 编码**：无 BOM 的 UTF-8 .ps1（含中文）会被按 ANSI 解析成乱码并破坏语法。脚本需写为 UTF-8 **带 BOM**（EF BB BF）。
- **npm 新策略**：esbuild 的 postinstall 被拦截会导致 Vite 失败，需 `npm rebuild esbuild`。
- **dev 期路径**：`tauri dev` 时 Rust 侧 cwd 是 `src-tauri`，而 SDK/资源在仓库根目录；`resolve_existing_path()` 依次尝试 原样→cwd→上级目录→exe 目录。

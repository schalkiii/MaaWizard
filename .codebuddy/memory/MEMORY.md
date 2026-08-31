# 长期记忆 (MEMORY.md)

## 项目：MaaWizard（规划名 MaaGeneral）

- 定位：基于 MaaFramework 的「可视化录制 + 节点编辑 + 一键真机运行」通用自动化 GUI（"Maa 版按键精灵"，零代码、大众向、图像识别抗变）。
- 仓库：`https://github.com/schalkiii/MaaWizard`，main 分支。`.gitignore` 排除 node_modules/dist/target/gen/maa-sdk/.codebuddy。推送用 `git -c user.name=<u> -c user.email=<u>@users.noreply.github.com` 一次性指定，不改全局 config。
- 技术栈：Rust + Tauri 2 + Vue3 + Vue Flow + 官方绑定 `maa-framework-rs`（crate `maa-framework`，feature `dynamic`，`load_library` 加载预编译库）。
- 后端模块：`maa/`(M0运行时)、`pipeline/`(PipelineDocument+V1/V2)、`recorder/`(inputbot采集+智能模板)、`capture/`(scrap截屏+ROI)、`device/`(Win32窗口列举)、`ai/`(环境探测)、`commands.rs`(命令层)。
- 前端：`App.vue` 三标签页(运行/图编辑器/录制)，ControllerPanel 已并入运行页。运行页含加载库/资源、连接(窗口或ADB)、状态每1.5s刷新、入口下拉+运行/停止、查看窗口画面(控制器截图)、识别回显(绿框，随 `maa://event` 回传节点名/命中/框[4]/命中风截图)。图编辑器：`graph.ts`映射层 + PipelineNodeView/JumpBackNodeView/GraphEditor + RoiCapture(抓模板) + NodeInspector。AI 页已从导航移除(代码保留可恢复)。
- 质量门禁：`make lint`=markdownlint-cli2(忽略maa-sdk/)+vue-tsc --noEmit+cargo clippy 全绿；`make test`=cargo test(44)+vitest(63) 共107全绿。前端 vitest+jsdom，Vue Flow 用替身文本断言；`src/test/setup.ts` 补 ResizeObserver。
- 规划文档：`docs/开发规划.md`；验收：`docs/验收清单.md`。

## 生态差异化（2026-08-31 实测）

- 5 个通用 GUI（MFAAvalonia/MXU/MFW-CFA/MaaFwApp/MWU）全是「PI V2 资源包运行器」，均不录制生成 Pipeline。本项目差异化=录制生成 Pipeline。但文档承诺的「导出 interface.json 供 MFAAvalonia 加载」代码 0 匹配，尚未实现，现状更像"第6个运行器"，需补导出能力。
- MaaFramework 本体 4744 stars/LGPL-3.0；合规仅限合法黑盒测试，禁外挂/破坏反作弊。
- 对我可用的 AI 工具：maafw-cli、MaaMCP、create-maa-project、Everything-Maa、MaaEvidenceKit、maaframework-skills。

## 构建与打包

- 本机无 make，Makefile 目标需手工等价：`make build`→`npm run tauri build`；`make check`→`cd src-tauri; cargo check`；`make fetch-sdk`→`powershell -NoProfile -ExecutionPolicy Bypass -File tools/fetch-maa-sdk.ps1`。
- **打包目标已定 nsis 单目标**（`tauri.conf.json` 的 `bundle.targets=["nsis"]`），仅产出 `.exe` 安装包，规避慢速 WiX(msi)。WiX 下载 GitHub >8min 会卡死，不再用 msi。
- 只要 exe：`npm run tauri build -- --no-bundle` 跳过打包（约 14MB 免安装）。
- 构建前必须关闭运行中的 exe，否则 patching 报「拒绝访问(os error 5)」。
- 长耗时构建/下载放后台：`Start-Process` + 日志轮询；清理卡住进程按 `Get-CimInstance Win32_Process` 的 CommandLine 过滤后 Stop-Process。
- npm 11 allow-scripts 拦截 esbuild/vue-demi 的 postinstall，但 vite 走平台包 `@esbuild/win32-x64` 不受影响，无需处理。
- Cargo.toml 假改动：git status 显示 M 但 `git hash-object` 与索引 blob 一致，是 autocrlf(LF→CRLF) 噪音，勿 `git checkout` 还原。
- maa-sdk（`maa-sdk/`，MAA-win-x86_64-v5.12.3）经 fetch-sdk 下载；dynamic 链接编译期不需，运行期 `load_library` 才需。

## 环境与编码踩坑（高复用）

- PowerShell 5.1 编码：无 BOM 的 UTF-8 .ps1（含中文）按 ANSI 误读→语法错（MissingEndCurlyBrace/字符串缺终止符）。脚本写 UTF-8 BOM 或纯 ASCII；`write_to_file` 新建含中文 .ps1 可能无 BOM，最稳写纯 ASCII。
- Tauri 图标：256×256 PNG 压缩 .ico 导致 RC.EXE 失败，须用 `Bitmap.GetHicon()` 生成标准 BMP 图标条目。
- MaaFramework 截图存盘：`cached_image().to_vec()`(GetEncoded) 常为空；优先 PNG 编码，空则回退 `raw_data()`(BGR(A)行主序) 构造 RGBA，封装为 `capture::save_maa_image`（控制器截图与运行态识别帧均走它）。
- Win32 控制器常量（致命）：`new_win32(hwnd, screencap, mouse, keyboard)` 后三参传 0=None(非自动)，导致 `post_screencap` 永远失败/status 0；须 `screencap=MaaWin32ScreencapMethod_All(-1)`、`mouse/keyboard=MaaWin32InputMethod_Seize(1)`。枚举在 `maa-framework-sys` 的 `static_bindings.rs`。
- asset 协议：前端 `convertFileSrc(path)` 需 `tauri.conf.json` 开 `app.security.assetProtocol.enable=true` + `scope`，且 `Cargo.toml` 的 `tauri` 加 `features=["protocol-asset"]`，否则 `<img>` broken。
- dev 期路径：`tauri dev` 时 Rust cwd 为 `src-tauri`，而 SDK/资源在仓库根；`resolve_existing_path()` 依次 原样→cwd→上级→exe 目录（最多 8 层）。
- 加载动态库：`load_library` 前 `SetDllDirectoryW(<DLL目录>)`，否则 `MaaFramework.dll` 找不到同目录的 MaaUtils/opencv/onnxruntime 依赖，报 LoadLibraryExW failed。
- 运行排查：点运行后先 `load_resource` 自动加载资源包；`run_task_blocking` 以 `MaaStatus::SUCCEEDED` 判成败（仅 wait 会误报成功）；入口节点名须存在于已加载资源包；`list_resources` 只认含 `pipeline/` 或 `image/` 的目录。
- 两种截图分工（不重复）：运行页『查看窗口画面』=`controller_screenshot`(仅目标窗口)；图编辑器/录制页 `RoiCapture`『截取屏幕』=`capture_desktop`(整屏框选ROI)。

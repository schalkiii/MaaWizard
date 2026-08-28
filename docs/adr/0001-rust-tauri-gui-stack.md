# GUI 技术栈采用 Rust + Tauri（而非官方推荐的 MFAAvalonia）

MaaWizard 的 GUI 外壳在 Rust + Tauri 2 + Vue3 + Vue Flow 与官方推荐的 MFAAvalonia（C#/Avalonia，MaaXYZ 组织）之间权衡。最终决定采用 Rust + Tauri。

- **原因**：(1) 项目已确定 Rust 技术路线，构建与录制/运行时逻辑统一为 Rust，避免双语言维护；(2) MaaInspector 已验证 Tauri(Rust)+Vue3+vue-flow 同栈可行；(3) 录制引擎、设备管理、运行时封装均为 Rust 逻辑，单一语言收益明显。
- **代价**：放弃与 MFAAvalonia 代码级复用，但通过遵循 **Project Interface V2 协议**保持生态互通（产出资源包可被 MFAAvalonia 加载）。
- **状态**：accepted

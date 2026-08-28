# PipelineDocument 以 Rust 后端为唯一真相源

图编辑器（前端 vue-flow）与录制引擎（Rust）都操作同一份 `PipelineDocument`。决定：`PipelineDocument` 作为 Rust 后端内存状态（由 Tauri 状态管理持有），前端仅持有通过 IPC 同步的镜像副本；所有变更经 Tauri command 提交后端，后端校验后通过事件回推刷新前端。

- **原因**：跨语言无法让前端直接修改 Rust `HashMap`；若前后端各自持有可变状态，保存/运行时会出现不一致。录制过程在 Rust 侧追加节点，前端需实时反映，故采用「后端真相 + 前端镜像 + command/event 同步」。
- **后果**：每个节点字段编辑都经一次 IPC 往返；高频录制（每秒多次事件）需批量合并或前端乐观更新 + 后端对账，避免卡顿。
- **状态**：accepted

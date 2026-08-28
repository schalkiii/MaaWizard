# MaaWizard 测试计划

> 配套：`docs/spec.md`（功能规格）、`docs/开发规划.md`（路线图）
> 测试分层：单元（Rust `cargo test` / 前端 `vitest`）→ 集成 → E2E。后端逻辑优先 Rust 单测，UI 交互用组件/集成测试。

---

## 1. 单元测试（Rust 后端）

### M0 运行时封装

- `load_library` 在库缺失/路径错误时返回明确错误，不 panic。
- `Resource::post_path` 对非法 Bundle（缺 `pipeline/`、含 `.`/`$` 前缀文件）按规则忽略且不错报。
- Controller 连接：Adb 错误地址、Win32 错误 hwnd 均返回可恢复错误。
- `focus` 回调能正确解析节点名 + 命中框 + 分数，并转为 Tauri Event 载荷。
- PI V2 解析：合法 `interface.json` 解析成功；缺字段/类型错误给出定位信息。

### PipelineDocument 模型（核心）

- **V1/V2 互转**：给定一份 V1 JSON，转 V2 后字段等价；再转回 V1 与原值一致（round-trip）。
- 序列化：Rust `PipelineDocument` ↔ on-disk JSON 字节级稳定（字段顺序、空值省略符合 MaaFramework 期望）。
- `next`/`on_error` 列表可混合字符串节点名与 `{type, param}` 对象；`[JumpBack]` 作为合法条目被保留。
- Schema 校验：非法节点（未知 recognition/action 类型、缺必填 `template`）被拦截并报错到字段。

### M2 录制引擎

- **step → node 转换**：
  - 给定一次鼠标点击 (x,y) + 截图 → 生成 `TemplateMatch(template=裁剪图) + Click`，且 Click 不携带非法 `target:"self"`（默认命中区）。
  - 给定一次滑动 (begin, end, duration) → 生成 `Swipe{begin,end,duration}`（坐标，非 TemplateMatch）。
  - 给定文本输入 → 生成 `InputText{input_text}`。
- **识别优化 pass**：ROI 内 OCR 高置信命中 → 改写为 `OCR(expected=文字)`；否则保持 `TemplateMatch`。需为"高置信"阈值设置可测边界（如 0.9）。
- **坐标模式**：生成 `DirectHit + roi(坐标)`，无识别节点，且被标注为脆弱模式。
- **串联**：连续 N 个 step 按时间顺序以 `next` 串联，入口为首节点；停止录制后 `PipelineDocument` 含 N 个节点。

### M3 模板 / ROI 抓取器

- ROI 矩形 → 合法 `roi` 字段；裁剪区域保存为 `image/` 下 PNG 并回填 `TemplateMatch.template` 路径。
- OCR 回填：`OCR.expected` 被自动包裹为正则（如 `^文字$`）。
- 取色：输出 `ColorMatch.lower/upper` 合法范围。

### M4 设备管理

- Adb 设备列表解析（`adb devices` 输出多种格式）；常见模拟器端口识别。
- Win32 窗口枚举与按标题匹配选择返回正确 hwnd。

### M6 AI（阶段 5）

- 外部子进程启动：Python 缺失时优雅降级（不崩溃、给出提示）。
- MCP/CLI 调用参数构造正确（命令、JSON 参数转义）。

---

## 2. 集成测试

- **M0 运行链路**：加载示例 Resource → 连接（先 Win32）→ `post_task` 跑通一个已知 pipeline，断言最终状态与预期节点命中一致。（需预置样例 Bundle + OCR 模型，见 fixtures）
- **录制→运行闭环**：模拟一段录制（注入合成输入事件 + 截图桩）→ 生成 PipelineDocument → 经 M0 加载运行 → 断言执行路径符合预期。
- **PI V2 互通**：用 MFAAvalonia 能识别的 `interface.json` 产出资源包，断言本项目可加载并运行（验证生态互通，不依赖实际启动 MFAAvalonia）。
- **多端**：Adb（连接模拟器/真机，若环境可用）与 Win32 各跑一条 smoke pipeline。

---

## 3. 前端 / E2E 测试（vitest + 组件测试）

- **图编辑器渲染**：给定 `PipelineDocument` 渲染出正确节点数与 `next`/`on_error` 连线；`[JumpBack]` 显示为回跳标记。
- **Inspector 动态表单**：选择不同 recognition/action 类型，表单字段随之变化；非法输入触发校验提示。
- **ROI 框选交互**：在截图上拖拽矩形 → 对应 `roi` 更新；一键导出模板 → 节点 `template` 填充。
- **使用指引**：选中 TemplateMatch 且无 template → 出现"请先框选截图生成模板"提示；选中 Shell 且控制器非 Adb → 提示不支持。
- **录制→编辑贯通**：启动录制 → 停止 → 编辑器出现对应节点（端到端冒烟）。

---

## 4. 测试夹具（fixtures）

- `tests/fixtures/sample_resource/`：最小可运行 Bundle（1 条 pipeline + 1 张模板图 + 占位 OCR 模型占位说明）。
- `tests/fixtures/pipeline_v1.json` / `pipeline_v2.json`：同一逻辑的两版表示，用于 round-trip 测试。
- `tests/fixtures/screenshots/`：含已知按钮/文字的截图，用于录制与 ROI/OCR 测试桩。
- 录制测试使用**截图桩 + 合成输入事件**，不依赖真实设备/显示器。

---

## 5. 覆盖目标与门禁

- 后端核心（PipelineDocument、M2 转换、M0 加载）单测覆盖率 ≥ 80%。
- CI 门禁：`cargo test` + `npm test` 全绿方可合并。
- 每个新识别/动作类型支持（M1）须配套至少 1 个序列化 + 1 个表单渲染测试。

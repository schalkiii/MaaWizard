# MaaWizard 功能规格说明 (Spec)

> 配套文档：`docs/开发规划.md`（架构、技术栈、路线图）
> 版本：v0.1（规格稿） ｜ 更新：2026-08-28
> 覆盖范围：模块功能清单、录制引擎→图编辑器串联、完整 maafw 能力支持、使用指引、maafw-cli/MaaMCP 反馈循环

---

## 0. 设计总览与核心数据流

```text
[录制引擎 M2] --生成--> PipelineDocument --绑定--> [图编辑器 M1]
                                              │
[模板抓取器 M3] --ROI/模板/OCR--> PipelineDocument
                                              │
[设备管理 M4] --连接--> [运行时 M0] --加载/运行--> PipelineDocument
                                              │
[调试回显 M5] <--focus 状态-- [运行时 M0]
                                              │
[AI 增强 M6] <--maafw-cli/MaaMCP--> 设备/生成/修正
```

**关键设计**：图编辑器（M1）与录制引擎（M2）、模板抓取器（M3）共享同一份 **`PipelineDocument`** Rust 数据模型。
录制结束即把录制的步骤**直接追加为 Pipeline 节点**并写入 `PipelineDocument`，编辑器随即渲染——实现"录完即得 pipeline"。

---

## 1. 共享数据模型 `PipelineDocument`

Rust 侧用 `serde` 定义，前后端通过 Tauri IPC 传输。覆盖 Pipeline V1/V2 双模式。

```rust
struct PipelineDocument {
    version: PipelineVersion,            // V1 | V2
    nodes: HashMap<String, PipelineNode>,// 节点名 -> 节点
    defaults: Option<DefaultPipeline>,   // default_pipeline.json 合并视图
    resource_root: PathBuf,              // 资源包根目录
}

struct PipelineNode {
    recognition: Recognition,            // 见 §2.1
    action: Action,                      // 见 §2.2
    next: Vec<NextEntry>,                // 后继节点（可混合字符串 / NodeAttr）
    on_error: Vec<NextEntry>,            // 超时/失败跳转
    // 节点级公共字段
    timeout: Option<i64>,
    rate_limit: Option<u64>,
    anchor: Option<Anchor>,
    inverse: Option<bool>,
    enabled: Option<bool>,
    max_hit: Option<u32>,
    pre_delay: Option<u64>,
    post_delay: Option<u64>,
    pre_wait_freezes: Option<u64>,
    post_wait_freezes: Option<u64>,
    repeat: Option<u32>,
    repeat_delay: Option<u64>,
    repeat_wait_freezes: Option<u64>,
    focus: Option<bool>,
    attach: Option<serde_json::Value>,
    // [JumpBack] 不是节点字段，而是 next/on_error 列表中的特殊标记字符串 "[JumpBack]"（v5.1 引入，替代废弃的 is_sub 字段）
    // is_sub(bool) 已在 v5.1 废弃，故不再建模为节点字段；回跳通过 next 列表中的 "[JumpBack]" 实现
}
```

---

## 2. 模块详细规格

### M0 运行时封装（Maa Runtime Wrapper）

对 `maa-framework-rs` 的薄封装，供其他模块调用。

| 功能 | 说明 |
| ------ | ------ |
| 加载库 | `load_library(path)` 加载预编译 `MaaFramework.dll/.so`；首次启动自动下载 OCR 模型 |
| 资源加载 | `Resource::post_path(resource_root)`；支持多资源包（base+mod）按 Dict Merge 合并 |
| 控制器连接 | **Adb**：`connect_adb(ip:port)` / 序列号；**Win32**：`connect_win32(hwnd/窗口名)`；返回控制器句柄 |
| 设备信息 | 截图、分辨率、设备型号（Adb）/ 窗口列表（Win32） |
| 任务执行 | `Tasker::post_task(entry_node)`；`stop()`；返回任务句柄 |
| 状态回调 | 注册 `focus` 回调，将当前节点名 + 识别结果（命中框、分数）通过 Tauri Event 推送给前端 |
| 校验 | 按控制器类型校验 action 参数合法性（如 Win32 专属 scroll、Adb 专属 shell） |
| 项目接口 (PI V2) | 解析并暴露 `interface.json`（Project Interface V2）：任务、选项、控制器、界面文本；使本工具产出的资源包可被 MFAAvalonia 等官方 GUI 直接加载 |
| 集成方案 | 采用官方推荐「方案2：JSON + 插件扩展（AgentServer 子进程）」；复杂逻辑经 `Custom` 节点 + Python AgentServer 扩展，保持与生态工具兼容 |

> **官方推荐对齐**：MaaFramework 事实上的推荐 GUI 是 **MFAAvalonia**（官方 `MaaXYZ` 组织，基于 Avalonia UI + SukiUI，消费 Project Interface V2 协议）。本项目 GUI 外壳采用 Rust + Tauri（与 MaaInspector 同栈），不直接使用 MFAAvalonia（C#），但**遵循 Project Interface V2 协议**作为资源/UI 解耦标准，从而纳入官方生态并与之兼容互通。

---

### M1 图编辑器（完整 maafw 支持 + 使用指引）

#### 1.1 识别（Recognition）类型完整支持

| 类型 | 关键参数 | 能实现的效果 | 典型用途 |
| ------ | --------- | ------------- | --------- |
| DirectHit | `roi` | 不识别，直接执行动作（常配合固定坐标或等待） | 固定位置点击、纯等待 |
| TemplateMatch | `template`, `threshold`, `method`, `green_mask`, `roi` | 在屏幕找图，命中后对其执行动作 | 按钮/图标定位（最常用） |
| FeatureMatch | `template`, `detector`(SIFT等), `ratio` | 抗透视/缩放找图 | 旋转、缩放场景 |
| ColorMatch | `lower`, `upper`, `method`, `connected` | 找色块，命中颜色区域 | 纯色状态指示、红绿灯 |
| OCR | `expected`(正则), `model`, `color_filter`, `roi` | 识字，按正则匹配文字 | 文字按钮、状态文案判断 |
| NeuralNetworkClassify | `model`, `labels`, `expected` | 固定位置分类（如图标类别） | 多状态图标区分 |
| NeuralNetworkDetect | `model`, `labels`, `expected` | YOLO 目标检测，定位多个目标 | 多目标场景 |
| And | `all_of: [识别列表]` | 多个识别同时命中才通过 | 复合条件 |
| Or | `any_of: [识别列表]` | 任一识别命中即通过 | 多候选按钮 |
| Custom | `custom_recognition`, `custom_recognition_param` | 自定义识别逻辑（Python/扩展） | 特殊算法 |

#### 1.2 动作（Action）类型完整支持

| 类型 | 关键参数 | 能实现的效果 |
| ------ | --------- | ------------- |
| DoNothing | — | 仅识别不操作（常用于判断/分支） |
| Click / LongPress | `target`, `contact`, `pressure` | 点击/长按识别到的目标中心 |
| Swipe / MultiSwipe | `begin`, `end`, `duration` | 滑动/多指滑动（含途径点） |
| TouchDown / TouchMove / TouchUp | 触控点 | 精细触控序列 |
| Scroll | `dx`, `dy`, `target` | 滚轮滚动（Win32） |
| ClickKey / LongPressKey / KeyDown / KeyUp | 键码 | 按键（依赖控制器键码表） |
| InputText | `input_text` | 文本输入 |
| StartApp / StopApp | `package` | 启停应用 |
| StopTask | — | 中断当前任务链 |
| Command | `exec`, `args`, `detach` | 执行外部程序 |
| Shell | `cmd` | ADB shell 命令 |
| Screencap | `filename`, `format` | 保存截图（调试/取证） |
| Custom | `custom_action` | 自定义动作 |

#### 1.3 编辑器交互

- **画布（vue-flow）**：节点拖拽、连线（`next` / `on_error` 两类边，区分颜色）、`[JumpBack]` 回跳标记、小地图、分组/便签。
- **节点检视器（Inspector）**：根据 recognition/action 类型**动态渲染参数表单**（图片选择器、ROI 输入、正则框、模型路径、阈值滑块等）。
- **右键菜单**：调试节点、查看任务链、复制/删除、设为入口。
- **Schema 校验**：保存前按官方 JSON Schema 校验，报错定位到字段。
- **V1/V2 互转**：导入旧项目自动识别并可在保存时选择目标协议版本。

#### 1.4 框选 ROI / 模板抓取（详见 M3）

- 截图后**拖拽矩形框选 ROI** → 写入对应字段的 `roi`。
- 框选区域可一键**导出为模板图片** → 自动填入 `TemplateMatch.template`。
- 框选区域可一键**OCR** → 自动填入 `OCR.expected`（正则）。

#### 1.5 使用指引系统（Help/Guide）

内置静态帮助注册表（Rust 或 JSON），每个识别/动作附带：

- **一句话效果**：该节点能实现什么。
- **参数说明**：每个参数的含义与取值建议（如 `threshold` 调高更严格、调低更易命中）。
- **常见坑**：如 TemplateMatch 需绿底图、`green_mask` 用法；OCR 需正确 `model`。
- **场景示例**：按钮点击、等待文字出现、多分支判断等模板化示例。
- **上下文提示**：例如选中 TemplateMatch 但未设 template → 提示"请先框选截图生成模板"；选中 Shell 但控制器非 Adb → 提示不支持。
- **快速上手引导**：首次启动的交互式教程（录制→编辑→运行三步）。

---

### M2 录制引擎（智能识别，直出 pipeline）

**核心目标**：录制结束即得到一份**带识别节点的 pipeline**（而非裸坐标），并直接载入图编辑器。

#### 2.1 智能录制流程（主模式）

监听全局输入（`inputbot`）并周期性截屏（`scrap`），每捕获一个用户操作即生成一个 pipeline 节点：

| 用户操作 | 生成节点 | 识别策略（智能） |
| --------- | --------- | ---------------- |
| 鼠标点击 | `TemplateMatch + Click` | 以点击点为中心自动裁剪 ROI → 存为模板图 → `template` 指向该图；`action: Click` 默认作用于识别命中区域（hitbox），无需显式 target |
| 文本框输入 | `DirectHit/OCR + InputText` | 先识别输入框（可选 OCR 定位），记录输入文本 → `action: InputText{input_text}` |
| 滑动 | `DirectHit + Swipe` | 记录 begin/end 坐标与 duration → `action: Swipe{begin, end, duration}`（`swipe` 为位置相关，默认用坐标；可选对 begin 做识别定位） |
| 按键 | `DirectHit + ClickKey` | 记录键码 → `action: ClickKey` |
| 停顿/等待 | `等待节点` | 若停顿超阈值，提示或自动插入"等待某元素出现"的识别节点 |

**录制后"识别优化"自动 pass**：

1. 对每个点击 ROI 尝试 OCR；若高置信命中文字 → 改为 `OCR` 节点（`expected`=文字）。
2. 否则保持 `TemplateMatch`（模板已裁剪保存）。
3. 用户可在编辑器或录制预览中**逐个覆盖**识别类型。

节点按时间顺序以 `next` 串联成链，入口为第一个节点。

#### 2.2 坐标录制（兜底模式）

- 同流程但使用 `DirectHit + roi(坐标)`，不生成识别节点。
- UI 明确标注为"坐标模式（抗变性差，建议仅用于固定界面）"。

#### 2.3 与图编辑器的串联机制（关键）

1. 录制开始时创建/清空一份 `PipelineDocument`（共享模型）。
2. 录制过程中每个操作 → 调用 `Recorder::push_step()` → 转换为 `PipelineNode` 追加进 `PipelineDocument.nodes`。
3. 点击"停止录制" → 录制引擎对 `PipelineDocument` 执行**识别优化 pass** → 通过 Tauri command 返回给前端。
4. 前端编辑器**直接绑定该 `PipelineDocument`** 并渲染为 vue-flow 节点/连线。
5. 用户在此基础上精修（改 ROI、换识别类型、加分支、`on_error`）→ "导出" 经 M0 序列化为 pipeline JSON。

> 串联本质：**Recorder 是 `PipelineDocument` 的生成器，Editor 是其查看/编辑视图**。二者解耦但共享同一份数据，故"录完即刻可编辑可运行"。

---

### M3 模板 / ROI 抓取器

| 功能 | 说明 |
| ------ | ------ |
| 截图 | 从当前控制器（`scrap` 或 M0 截图）获取当前画面 |
| ROI 框选 | 鼠标拖拽矩形 → 输出 `roi` 坐标（支持相对/绝对，适配分辨率变化） |
| 模板导出 | 框选区域裁剪 → 保存到 `resource/image/` → 回填 `TemplateMatch.template` 路径 |
| 文字识别 | 框选区域 OCR → 回填 `OCR.expected`（自动包裹为正则） |
| 取色 | 框选/取点 → 输出 `ColorMatch.lower/upper` |
| 绿幕工具集成 | 调用/对接 Auto Green Background，生成 `green_mask` 友好模板 |

---

### M4 设备管理

| 功能 | 说明 |
| ------ | ------ |
| ADB 设备 | 列出 `adb devices`、连接 `ip:port`、识别常见模拟器端口 |
| Win32 窗口 | 枚举窗口列表、按标题/类名选择、获取 hwnd |
| 连接预览 | 连接后显示实时截图缩略图、分辨率 |
| 参数校验 | 按控制器类型提示不支持的 action（参考 M0 校验） |
| 多设备 | 支持同时连接多设备，切换目标 |

---

### M5 调试与状态回显

| 功能 | 说明 |
| ------ | ------ |
| 实时高亮 | Tasker `focus` 回调 → 前端高亮当前执行节点 |
| 结果叠加 | 在截图上叠加识别命中框、分数、ROI（可视化调试） |
| 单节点调试 | 右键"调试节点"单独执行某节点并查看结果 |
| 日志 | 每节点执行日志、识别耗时、失败原因 |
| 停止 | 运行中可 `post_stop` 中止 |

---

### M6 AI 增强与反馈迭代循环

#### 6.1 maafw-cli / MaaMCP 在本项目中的角色

二者是把"AI（含本助手）"接入设备与 MaaFramework 的现成通道，**开发期即可复用，无需自研设备驱动**：

- **maafw-cli（Python CLI，token 省）**
  - `connect adb/win32` → `reco`/`ocr` 观察真实屏幕；`click e3` 用 Element 引用避免重复识别。
  - 用途：开发期由 AI 探索目标 App、验证识别参数、生成候选节点。

- **MaaMCP（MCP Server）**
  - 工具：`find_adb_device_list` / `connect_*` / `ocr` / `click` / `save_pipeline` / `load_pipeline` / `run_pipeline` / `start_pipeline`（流水线后台监控模式）。
  - 用途：作为 AI Agent 的标准化设备驱动；`run_pipeline`/`save_pipeline` 可直接读写本项目产出的 pipeline。

- **create-maa-project / Everything-Maa（Skills + MCP 配置）**
  - 脚手架与维护 Maa 项目；AI Skill 覆盖意图编排→pipeline 编写/生成/测试全流程。

**本项目对它们的双向使用**：

1. 开发期：AI 用 maafw-cli/MaaMCP **探查设备、生成/校验节点**，导入到 MaaWizard。
2. 运行期（M6）：MaaWizard 可**内置启动 MaaMCP/maafw-cli 子进程**作为 AI 生成能力的后端，使"自然语言→pipeline"在应用内可用。

#### 6.2 反馈迭代循环（Feedback Loop）

构建"录制/生成 → 运行 → 观测 → 分析 → 修正"的自改进闭环：

```text
① 生成  用户录制(M2) 或 AI 生成(M6/maafw-cli) → 得到草稿 pipeline
   │
② 运行  M0 在真实设备执行（或经 maafw-cli/MaaMCP 运行）
   │
③ 观测  M5 收集：节点状态、识别命中框/分数、截图、日志
   │      （可经 MaaEvidenceKit 式证据抽取，得到确定性失败点）
   │
④ 分析  AI(经 maafw-cli/MaaMCP 读证据)：定位失败（节点未命中 /
   │      模板不准 / OCR 误匹配 / next 链错误）
   │
⑤ 修正  AI 编辑 pipeline：换识别类型、调 threshold/roi、补 on_error、
   │      修正分支 → 回到 ①
   │
⑥ 沉淀  成功模式 → 模板库 / Everything-Maa Skills 复用
```

**闭环价值**：

- 录制/AI 负责"生产"，运行/观测负责"验证"，AI 负责"诊断+修正"，形成可自我完善的自动化构建回路。
- 与 MaaWizard 自身运行证据打通后，可实现"运行失败 → 自动诊断 → 建议修复"的体验，大幅降低非开发者调参成本。

---

## 3. 模块间职责边界（避免重复）

- **设备控制**：统一由 M0（maa-framework-rs Controller）负责；M2/M4/M6 均不直接调 ADB，统一走 M0。
- **节点生成**：M2（录制）、M3（ROI 抓取）、M6（AI）都只产出/修改 `PipelineDocument`，不直接碰运行时。
- **渲染**：M1 只消费 `PipelineDocument`，不持有业务逻辑。
- **识别能力**：OCR/模板匹配统一用 MaaFramework，不另起炉灶。

---

## 4. 合规与边界

- 仅用于合法自动化黑盒测试；UI 明确声明禁止外挂与破坏反作弊机制（遵循 LGPL-3.0）。
- `Command`/`Shell` 节点在 UI 层增加安全提示，避免误执行外部命令。
- 录制产生的模板图片、日志不含敏感信息；导出包不含密钥/Token。

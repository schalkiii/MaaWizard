# MaaWizard 术语表（Glossary）

MaaWizard 是基于 MaaFramework 的「可视化录制 + 节点编辑 + 一键运行」通用自动化 GUI。本表仅收录本项目特有的领域概念，通用编程术语（timeout、error 等）不收录。

## 语言（术语）

**Pipeline（流水线 / 任务流水线）**：用 JSON 描述的自动化任务图，由若干 Node 组成；是 MaaFramework 的核心执行单元。
_Avoid_：脚本、工作流（易与编辑器内的"画布/工作流"混淆）

**Node（节点）**：Pipeline 中的一个单元，含一次识别（Recognition）与一次动作（Action），以及流转到下一节点的 `next`/`on_error`。
_Avoid_：步骤、块

**Recognition（识别）**：节点中决定"是否命中、命中在哪里"的算法，如 TemplateMatch / OCR / ColorMatch。
_Avoid_：检测

**Action（动作）**：节点中识别命中后要执行的设备操作，如 Click / Swipe / InputText；默认作用于本次识别命中区域（hitbox）。
_Avoid_：操作

**Controller（控制器）**：连接并驱动真实设备的抽象，分 Adb（Android）与 Win32（Windows 窗口）等。
_Avoid_：设备驱动

**Resource（资源包 / Bundle）**：包含 `pipeline/`、`image/`、`model/` 的目录，经 `Resource::post_path` 加载后供 Tasker 使用。
_Avoid_：工程

**Tasker（任务调度器）**：MaaFramework 中真正调度执行 Pipeline 的组件，持有 Resource 与 Controller。
_Avoid_：执行器

**Project Interface / PI V2（项目接口）**：描述任务、选项、控制器、界面文本的 JSON（`interface.json`），使任意 GUI（含 MFAAvalonia）能驱动本资源包。
_Avoid_：配置

**ROI（感兴趣区域）**：识别时的搜索框（矩形），是搜索范围而非点击点；区别于"点击坐标"。
_Avoid_：选区

**Template（模板）**：TemplateMatch 所需的参考图片，通常由录制/抓取时裁剪保存。
_Avoid_：样本图

**PipelineDocument（管线文档）**：本项目内部 Rust 模型，是 Pipeline 的内存表示，供录制引擎生成、编辑器渲染、运行时加载。
_Avoid_：文档（易误解为普通文件）

**[JumpBack]**：写在 `next`/`on_error` 列表中的特殊标记字符串，表示链路走空时回跳到跳转栈记录的节点；**不是节点字段**，`is_sub`（已废弃）亦非必填。
_Avoid_：回退节点

**AgentServer（插件扩展子进程）**：官方推荐「集成方案 2」中托管复杂识别/动作逻辑的外部进程，打破语言壁垒。
_Avoid_：插件

**Element（maafw-cli 元素引用）**：maafw-cli 中把 OCR/识别结果赋予 `e1,e2…`，后续 `click e3` 复用，避免重复识别。
_Avoid_：变量

## 关系

- 一个 **Resource（Bundle）** 包含多条 **Pipeline**，每条 Pipeline 由若干 **Node** 组成。
- 一个 **Node** 含一次 **Recognition** 与一次 **Action**，经 `next` 指向其他 **Node**（可含 **[JumpBack]**）。
- **Tasker** 持有 **Resource** 与 **Controller**，调度执行 **Pipeline**。
- **PipelineDocument** 是 **Pipeline** 在应用内的可编辑镜像；导出后成为 **Resource** 中的 pipeline JSON。
- 一个 **Resource** 可附带 **Project Interface V2**，使外部 GUI 也能驱动。

## 示例对话

> **开发者**："录制得到的是 PipelineDocument，为什么导出后还要 Resource？"
> **领域专家**："PipelineDocument 只是内存里的可编辑模型；导出写成 `pipeline/*.json` 放进 Resource 目录，Tasker 才能 `post_path` 加载运行。"
>
> **开发者**："ROI 是不是就是点击坐标？"
> **领域专家**："不是。ROI 只是识别的搜索框；真正点击的位置来自 Recognition 的命中框（hitbox），所以换分辨率也能命中。"

## 已标记的歧义

- "抗变"曾被人误读为"坐标归一化适配分辨率"——已澄清为"依赖重识别而非坐标缩放"（见 ADR 0003）。
- `target:"self"` 曾被误写入 Click 动作——MaaFramework 无此语法，动作默认作用于识别命中区（spec 已修正）。
- `[JumpBack]` 曾被建模为节点字段——实为 `next` 列表中的特殊标记字符串（spec 已修正）。

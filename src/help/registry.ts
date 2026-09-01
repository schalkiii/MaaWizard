/**
 * 使用指引注册表。
 * 为每种识别/动作说明「能实现什么效果」「参数含义」「常见坑」，
 * 对应 spec 中 M1 的使用指引系统（§1.5）。
 */

export interface ParamHelp {
  name: string;
  desc: string;
}

export interface HelpEntry {
  /** 一句话效果 */
  effect: string;
  /** 典型用途 */
  scene: string;
  params: ParamHelp[];
  /** 常见坑与注意事项 */
  tips?: string[];
}

export const RECOGNITION_HELP: Record<string, HelpEntry> = {
  DirectHit: {
    effect: "不识别，直接执行动作。",
    scene: "固定位置点击、纯等待、作为流程起点",
    params: [{ name: "roi", desc: "限定区域 [x, y, w, h]，不填则为全屏" }],
    tips: ["不做识别，因此抗变性最差，换分辨率就会失效"],
  },
  TemplateMatch: {
    effect: "在画面中「找图」，命中后对命中区域执行动作。",
    scene: "按钮、图标、固定样式元素的定位（最常用）",
    params: [
      { name: "template", desc: "模板图片路径，相对于资源包 image/ 目录" },
      { name: "threshold", desc: "匹配阈值，调高更严格、调低更易命中（常用 0.7~0.9）" },
      { name: "method", desc: "匹配算法，默认使用 OpenCV 模板匹配" },
      { name: "green_mask", desc: "对绿幕模板做抠图，需配合涂绿幕工具使用" },
      { name: "roi", desc: "只在指定区域内搜索，可显著提升速度与准确率" },
    ],
    tips: [
      "模板图应在目标分辨率下截取，跨分辨率差异大时需重新录制",
      "点击目标由识别命中框决定，因此换分辨率后仍能命中（不是写死坐标）",
    ],
  },
  FeatureMatch: {
    effect: "基于特征点找图，抗透视与缩放。",
    scene: "存在旋转、缩放、轻微形变的场景",
    params: [
      { name: "template", desc: "模板图片路径" },
      { name: "detector", desc: "特征检测器，如 SIFT" },
      { name: "ratio", desc: "特征匹配比例阈值" },
    ],
    tips: ["比 TemplateMatch 慢，普通按钮优先用 TemplateMatch"],
  },
  ColorMatch: {
    effect: "在画面中「找色块」。",
    scene: "纯色状态指示、红绿灯、血条颜色判断",
    params: [
      { name: "lower", desc: "颜色下界 [r, g, b]" },
      { name: "upper", desc: "颜色上界 [r, g, b]" },
      { name: "connected", desc: "是否要求颜色区域连通" },
    ],
  },
  OCR: {
    effect: "识别画面中的文字，并按正则匹配。",
    scene: "文字按钮、状态文案判断、需要读取动态数值",
    params: [
      { name: "expected", desc: "匹配的正则/字符串，如 ^开始$" },
      { name: "model", desc: "OCR 模型目录，不填则用默认模型" },
      { name: "roi", desc: "只在指定区域识别，能大幅提速并减少误匹配" },
      { name: "color_filter", desc: "按颜色过滤文字，减少背景干扰" },
    ],
    tips: [
      "expected 是正则，含特殊字符需转义",
      "点击目标取自识别到的文字框，因此同样具备抗变性",
    ],
  },
  NeuralNetworkClassify: {
    effect: "对固定位置做图像分类。",
    scene: "同一位置出现多种状态图标时区分状态",
    params: [
      { name: "model", desc: "分类模型路径" },
      { name: "labels", desc: "标签列表" },
      { name: "expected", desc: "期望的标签" },
    ],
  },
  NeuralNetworkDetect: {
    effect: "用目标检测模型定位画面中的多个目标。",
    scene: "需要同时定位多个同类目标的复杂场景",
    params: [
      { name: "model", desc: "检测模型路径（如 YOLO）" },
      { name: "labels", desc: "标签列表" },
      { name: "expected", desc: "期望的标签" },
    ],
  },
  And: {
    effect: "多个识别「同时命中」才通过。",
    scene: "复合条件判断，例如同时出现图标和文字",
    params: [{ name: "all_of", desc: "子识别列表，全部命中才算命中" }],
  },
  Or: {
    effect: "任一识别命中即通过。",
    scene: "多候选元素，例如按钮可能有两种样式",
    params: [{ name: "any_of", desc: "子识别列表，任一命中即命中" }],
  },
  Custom: {
    effect: "调用自定义识别逻辑。",
    scene: "内置算法无法满足的特殊识别需求",
    params: [
      { name: "custom_recognition", desc: "自定义识别器名称" },
      { name: "custom_recognition_param", desc: "传给自定义识别器的参数" },
    ],
  },
};

export const ACTION_HELP: Record<string, HelpEntry> = {
  DoNothing: {
    effect: "仅做识别，不执行操作。",
    scene: "纯判断节点，配合 next 实现分支",
    params: [],
  },
  Click: {
    effect: "点击本次识别命中的位置。",
    scene: "点击按钮、选项等（最常用）",
    params: [
      {
        name: "target",
        desc:
          "点击目标：省略 / \"Self\"=识别命中处；[x,y]=固定坐标；[x,y,w,h]=区域；\"PreTask\"=上一节点命中处",
      },
      { name: "target_offset", desc: "在 target 基础上的偏移矩形 [x, y, w, h]" },
    ],
    tips: [
      "默认点击识别命中框，无需手写坐标，因此具备抗变性",
      "填写示例：留空 {} 即点命中处；{\"target\":[100,200]} 点固定坐标；{\"target_offset\":[5,5,10,10]} 微调",
    ],
  },
  LongPress: {
    effect: "长按识别命中的位置。",
    scene: "需要长按触发的菜单、拖拽起点",
    params: [
      { name: "duration", desc: "长按持续时间（毫秒）" },
      { name: "target", desc: "长按目标，默认识别命中区域" },
    ],
  },
  Swipe: {
    effect: "从起点滑动到终点。",
    scene: "翻页、列表滚动、解锁滑块",
    params: [
      { name: "begin", desc: "起点 [x, y]" },
      { name: "end", desc: "终点 [x, y]，也可传多个途经点" },
      { name: "duration", desc: "滑动耗时（毫秒）" },
    ],
    tips: ["滑动是位置相关的，通常直接使用坐标"],
  },
  MultiSwipe: {
    effect: "多指同时滑动。",
    scene: "需要多指手势的场景（缩放等）",
    params: [{ name: "begin", desc: "各指起点列表" }, { name: "end", desc: "各指终点列表" }],
  },
  InputText: {
    effect: "输入一段文本。",
    scene: "在输入框中填写内容",
    params: [{ name: "input_text", desc: "要输入的文本" }],
  },
  ClickKey: {
    effect: "按下并松开一个按键。",
    scene: "回车确认、快捷键、功能键操作",
    params: [{ name: "key", desc: "键码（Adb 用 Android keycode，Win32 用虚拟键码 VK）" }],
    tips: ["键码随控制器类型不同，录制时会自动按 Win32 VK 记录"],
  },
  StartApp: {
    effect: "启动应用。",
    scene: "流程开始前拉起目标应用",
    params: [{ name: "package", desc: "包名（Adb）或应用标识（Win32）" }],
  },
  StopApp: {
    effect: "停止应用。",
    scene: "流程结束后关闭目标应用",
    params: [{ name: "package", desc: "包名" }],
  },
  StopTask: {
    effect: "中断当前任务链。",
    scene: "检测到终止条件时主动结束流程",
    params: [],
  },
  Scroll: {
    effect: "滚动滚轮。",
    scene: "桌面端页面滚动（主要支持 Win32）",
    params: [
      { name: "dx", desc: "水平滚动量，正数向右" },
      { name: "dy", desc: "垂直滚动量，正数向下" },
    ],
  },
  Command: {
    effect: "执行外部程序。",
    scene: "调用外部脚本或工具",
    params: [
      { name: "exec", desc: "可执行文件路径" },
      { name: "args", desc: "参数列表" },
      { name: "detach", desc: "是否后台运行" },
    ],
    tips: ["⚠ 会执行外部程序，请确认来源可信"],
  },
  Shell: {
    effect: "执行 ADB shell 命令。",
    scene: "Android 设备上执行 shell 指令",
    params: [{ name: "cmd", desc: "shell 命令" }],
    tips: ["仅 Adb 控制器支持，Win32 下不可用"],
  },
  Screencap: {
    effect: "保存当前截图。",
    scene: "调试与取证",
    params: [
      { name: "filename", desc: "保存的文件名" },
      { name: "format", desc: "图片格式" },
    ],
  },
  Custom: {
    effect: "调用自定义动作。",
    scene: "内置动作无法满足的特殊操作",
    params: [{ name: "custom_action", desc: "自定义动作名称" }],
  },
  TouchDown: {
    effect: "按下屏幕（不抬起），用于组合复杂手势。",
    scene: "需要精确控制按下/移动/抬起的拖拽",
    params: [
      { name: "contact", desc: "触点编号（多指时区分不同手指），从 0 开始" },
      {
        name: "target",
        desc: "同上：省略 / \"Self\"=命中处；[x,y]=坐标；[x,y,w,h]=区域",
      },
      { name: "target_offset", desc: "偏移矩形 [x, y, w, h]" },
    ],
  },
  TouchMove: {
    effect: "移动已按下的触点。",
    scene: "拖拽过程的中间移动",
    params: [
      { name: "contact", desc: "触点编号" },
      { name: "target", desc: "目标位置，同 Click" },
      { name: "target_offset", desc: "偏移矩形 [x, y, w, h]" },
    ],
  },
  TouchUp: {
    effect: "抬起指定触点，结束一次触摸。",
    scene: "拖拽结束",
    params: [{ name: "contact", desc: "触点编号" }],
  },
  LongPressKey: {
    effect: "长按一个按键。",
    scene: "需要长按触发的快捷键",
    params: [
      { name: "key", desc: "键码（Adb 用 Android keycode，Win32 用虚拟键码 VK）" },
      { name: "duration", desc: "长按时长（毫秒），默认 1000" },
    ],
  },
  KeyDown: {
    effect: "仅按下按键（不抬起）。",
    scene: "组合键中的修饰键",
    params: [{ name: "key", desc: "键码" }],
  },
  KeyUp: {
    effect: "仅抬起按键。",
    scene: "配合 KeyDown 完成组合键",
    params: [{ name: "key", desc: "键码" }],
  },
};

/** 各识别/动作在选中时的推荐默认参数（JSON 字符串），用于输入框预填「尚可的默认值」 */
export const RECOGNITION_DEFAULT: Record<string, string> = {
  DirectHit: "{}",
  TemplateMatch: '{\n  "template": "your_template.png",\n  "threshold": 0.8\n}',
  FeatureMatch: '{\n  "template": "your_template.png"\n}',
  ColorMatch: '{\n  "lower": [0, 0, 0],\n  "upper": [255, 255, 255]\n}',
  OCR: '{\n  "expected": "^开始$"\n}',
  NeuralNetworkClassify: '{\n  "model": "model.onnx",\n  "labels": ["label1"]\n}',
  NeuralNetworkDetect: '{\n  "model": "model.onnx",\n  "labels": ["label1"]\n}',
  And: '{\n  "all_of": []\n}',
  Or: '{\n  "any_of": []\n}',
  Custom: '{\n  "custom_recognition": "name"\n}',
};

export const ACTION_DEFAULT: Record<string, string> = {
  DoNothing: "{}",
  Click: "{}",
  LongPress: '{\n  "duration": 1000\n}',
  Swipe: '{\n  "begin": [0, 0],\n  "end": [0, 0]\n}',
  MultiSwipe: '{\n  "swipes": []\n}',
  TouchDown: '{\n  "contact": 0,\n  "target": [0, 0]\n}',
  TouchMove: '{\n  "contact": 0,\n  "target": [0, 0]\n}',
  TouchUp: '{\n  "contact": 0\n}',
  Scroll: '{\n  "dx": 0,\n  "dy": -120\n}',
  ClickKey: '{\n  "key": 13\n}',
  LongPressKey: '{\n  "key": 13\n}',
  KeyDown: '{\n  "key": 13\n}',
  KeyUp: '{\n  "key": 13\n}',
  InputText: '{\n  "input_text": "hello"\n}',
  StartApp: '{\n  "package": "com.example"\n}',
  StopApp: '{\n  "package": "com.example"\n}',
  StopTask: "{}",
  Command: '{\n  "exec": "cmd.exe"\n}',
  Shell: '{\n  "cmd": "ls"\n}',
  Screencap: '{\n  "filename": "shot.png"\n}',
  Custom: '{\n  "custom_action": "name"\n}',
};

/** 选中某识别/动作时，若当前参数为空或仅 {}，则预填推荐默认值 */
export function defaultParam(kind: "recognition" | "action", type: string): string {
  const table = kind === "recognition" ? RECOGNITION_DEFAULT : ACTION_DEFAULT;
  return table[type] ?? "{}";
}

/** 节点级公共字段的指引 */
export const NODE_FIELD_HELP: ParamHelp[] = [
  { name: "next", desc: "识别成功后依次尝试的后继节点，命中第一个即停止" },
  { name: "on_error", desc: "识别超时或动作失败时跳转的节点" },
  { name: "timeout", desc: "等待 next 命中的超时（毫秒），默认 20 秒，-1 为无限" },
  { name: "rate_limit", desc: "两次识别的最小间隔（毫秒），默认 1000" },
  { name: "inverse", desc: "反转识别结果，改为「未命中才通过」" },
  { name: "enabled", desc: "是否启用该节点" },
  { name: "max_hit", desc: "最大命中次数，达到后跳过该节点" },
  { name: "pre_delay", desc: "动作执行前的延迟（毫秒）" },
  { name: "post_delay", desc: "动作执行后的延迟（毫秒）" },
  { name: "repeat", desc: "动作重复执行次数" },
  { name: "focus", desc: "是否向 UI 上报该节点状态（用于调试高亮）" },
];

export const RECOGNITION_TYPES = Object.keys(RECOGNITION_HELP);
export const ACTION_TYPES = Object.keys(ACTION_HELP);

/**
 * 根据选择的识别/动作与控制器类型给出上下文提示。
 * 对应 spec §1.5 的「上下文提示」。
 */
export function contextualHints(
  recognitionType: string,
  actionType: string,
  controller: string,
): string[] {
  const hints: string[] = [];

  if (recognitionType === "TemplateMatch" ) {
    hints.push("请先通过框选截图生成模板图，再填入 template 字段");
  }
  if (actionType === "Shell" && controller !== "adb") {
    hints.push("⚠ Shell 仅支持 Adb 控制器，当前控制器下该节点不会生效");
  }
  if (actionType === "Scroll" && controller !== "win32") {
    hints.push("⚠ Scroll 主要支持 Win32 控制器");
  }
  if (recognitionType === "OCR") {
    hints.push("expected 为正则表达式，含 . * ? 等字符时需转义");
  }

  return hints;
}

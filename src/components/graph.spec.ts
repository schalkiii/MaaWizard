import { describe, expect, it } from "vitest";

import type { PipelineDocument, ValidationIssue } from "../api/maa";
import {
  buildEdges,
  buildLayout,
  buildNodes,
  countIssues,
  describeSpec,
  entryNode,
  entryTarget,
  hasJumpBack,
  JUMPBACK_ID,
  recognitionColor,
} from "./graph";

/** Start 无入边，是入口；Retry 汇回 Step2，用于验证合并入度 */
const sample: PipelineDocument = {
  Start: {
    recognition: "TemplateMatch",
    action: "Click",
    next: ["Step2"],
    on_error: ["Retry"],
  },
  Step2: {
    recognition: { type: "OCR", param: { expected: "^开始$" } },
    action: "InputText",
    next: [{ name: "End" }],
  },
  Retry: { recognition: "DirectHit", action: "DoNothing", next: ["Step2"] },
  End: { recognition: "DirectHit", action: "DoNothing" },
};

describe("协议兼容解析", () => {
  it("describeSpec 兼容 V1 字符串与 V2 对象", () => {
    expect(describeSpec("TemplateMatch")).toBe("TemplateMatch");
    expect(describeSpec({ type: "OCR", param: { expected: "a" } })).toBe("OCR");
    expect(describeSpec(undefined)).toBe("-");
  });

  it("entryTarget 支持字符串与对象两种写法", () => {
    expect(entryTarget("Next")).toBe("Next");
    expect(entryTarget({ name: "Next" })).toBe("Next");
    expect(entryTarget({ foo: 1 })).toBeNull();
  });
});

describe("buildLayout 自动布局", () => {
  it("入口节点排在最左列", () => {
    const layout = buildLayout(sample);
    expect(layout.get("Start")?.x).toBe(0);
    expect(layout.get("Step2")?.x).toBeGreaterThan(0);
  });

  it("同层节点纵向错开", () => {
    const layout = buildLayout({ A: { next: ["C"] }, B: { next: ["C"] }, C: {} });
    expect(layout.get("A")?.x).toBe(layout.get("B")?.x);
    expect(layout.get("A")?.y).not.toBe(layout.get("B")?.y);
  });

  it("循环引用不会死循环，且节点不丢失", () => {
    const layout = buildLayout({ A: { next: ["B"] }, B: { next: ["A"] } });
    expect(layout.size).toBe(2);
  });

  it("自环不会产生悬空节点", () => {
    const layout = buildLayout({ A: { next: ["A"] } });
    expect(layout.size).toBe(1);
  });

  it("空文档返回空布局", () => {
    expect(buildLayout({}).size).toBe(0);
  });
});

describe("buildNodes 节点构建", () => {
  it("每个节点生成画布节点并带上识别/动作信息", () => {
    const nodes = buildNodes(sample);
    expect(nodes).toHaveLength(4);
    const start = nodes.find((node) => node.id === "Start")!;
    expect(start.type).toBe("pipeline");
    expect(start.data.recognition).toBe("TemplateMatch");
    expect(start.data.action).toBe("Click");
  });

  it("入口节点被标记出来且唯一", () => {
    const entries = buildNodes(sample)
      .filter((node) => node.data.isEntry)
      .map((node) => node.id);
    expect(entries).toEqual(["Start"]);
  });

  it("已保存的位置优先于自动布局", () => {
    const nodes = buildNodes(sample, { Start: { x: 999, y: 111 } });
    expect(nodes.find((node) => node.id === "Start")?.position).toEqual({ x: 999, y: 111 });
  });

  it("校验问题汇总成节点角标", () => {
    const issues: ValidationIssue[] = [
      { node: "Start", level: "error", field: "recognition.template", message: "缺少 template" },
      { node: "Start", level: "warning", field: "recognition.roi", message: "脆弱" },
      { node: "", level: "warning", field: "", message: "文档级别问题不计入节点" },
    ];
    const nodes = buildNodes(sample, {}, issues);

    const start = nodes.find((node) => node.id === "Start")!;
    expect(start.data.errors).toBe(1);
    expect(start.data.warnings).toBe(1);
    expect(
      nodes.filter((node) => node.id !== "Start").every((node) => node.data.errors === 0),
    ).toBe(true);
  });

  it("存在 [JumpBack] 时追加回跳标记节点", () => {
    const nodes = buildNodes({ A: { next: ["[JumpBack]"] } });
    expect(nodes.map((node) => node.id)).toContain(JUMPBACK_ID);
    expect(nodes.find((node) => node.id === JUMPBACK_ID)?.type).toBe("jumpback");
  });
});

describe("buildEdges 连线构建", () => {
  it("next 与 on_error 语义分开", () => {
    const edges = buildEdges(sample);
    const next = edges.find((edge) => edge.source === "Start" && edge.target === "Step2")!;
    expect(next.kind).toBe("next");
    expect(next.animated).toBe(true);

    const onError = edges.find((edge) => edge.source === "Start" && edge.target === "Retry")!;
    expect(onError.kind).toBe("on_error");
    expect(onError.style.strokeDasharray).toBe("5 5");
  });

  it("对象形式的后继取 name 作为目标", () => {
    expect(buildEdges(sample).some((e) => e.source === "Step2" && e.target === "End")).toBe(true);
  });

  it("[JumpBack] 指向合成节点", () => {
    expect(buildEdges({ A: { next: ["[JumpBack]"] } })[0].target).toBe(JUMPBACK_ID);
  });

  it("next 与 on_error 指向同一节点时 id 不冲突", () => {
    const edges = buildEdges({ A: { next: ["B"], on_error: ["B"] } });
    expect(new Set(edges.map((edge) => edge.id)).size).toBe(2);
  });

  it("无法解析的条目被忽略", () => {
    expect(buildEdges({ A: { next: [{ foo: 1 }] } })).toHaveLength(0);
  });
});

describe("hasJumpBack / entryNode", () => {
  it("识别 next 与 on_error 中的回跳标记", () => {
    expect(hasJumpBack({ A: { next: ["[JumpBack]"] } })).toBe(true);
    expect(hasJumpBack({ A: { on_error: ["[JumpBack]"] } })).toBe(true);
    expect(hasJumpBack(sample)).toBe(false);
  });

  it("入口是没有入边的节点", () => {
    expect(entryNode(sample)).toBe("Start");
    expect(entryNode({})).toBeNull();
    // 互相指向时没有严格入口
    expect(entryNode({ A: { next: ["B"] }, B: { next: ["A"] } })).toBeNull();
  });
});

describe("countIssues / recognitionColor", () => {
  it("按节点与级别汇总问题", () => {
    const counts = countIssues([
      { node: "A", level: "error", field: "", message: "" },
      { node: "A", level: "error", field: "", message: "" },
      { node: "B", level: "warning", field: "", message: "" },
      { node: "", level: "error", field: "", message: "" },
    ]);
    expect(counts.get("A")).toEqual({ errors: 2, warnings: 0 });
    expect(counts.get("B")).toEqual({ errors: 0, warnings: 1 });
    expect(counts.has("")).toBe(false);
  });

  it("已知类型有配色，未知类型有兜底色", () => {
    expect(recognitionColor("TemplateMatch")).toBe("#2563eb");
    expect(recognitionColor("OCR")).toBe("#059669");
    expect(recognitionColor("不存在的类型")).toBe("#64748b");
  });
});

import { mount, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent, h } from "vue";

import type { PipelineDocument, ValidationIssue } from "../api/maa";
import GraphEditor from "./GraphEditor.vue";
import { JUMPBACK_ID, type NodePosition, type PipelineGraphEdge, type PipelineGraphNode } from "./graph";

/**
 * 用轻量替身替换 Vue Flow：jsdom 没有布局能力，
 * 这里把节点/连线渲染成可断言的文本，并暴露 data-* 便于检查节点元数据。
 */
const VueFlowStub = defineComponent({
  name: "VueFlowStub",
  props: {
    nodes: { type: Array, required: true },
    edges: { type: Array, required: true },
  },
  emits: ["connect", "nodeClick", "edgeClick", "nodeDragStop"],
  setup(props, { emit }) {
    return () =>
      h("div", [
        ...(props.nodes as unknown as PipelineGraphNode[]).map((node) =>
          h(
            "span",
            {
              class: "node",
              "data-id": node.id,
              "data-type": node.type,
              "data-errors": String(node.data.errors),
              "data-entry": String(node.data.isEntry),
              onClick: () => emit("nodeClick", { node: { id: node.id } }),
            },
            node.data.name,
          ),
        ),
        ...(props.edges as unknown as PipelineGraphEdge[]).map((edge) =>
          h(
            "span",
            {
              class: "edge",
              "data-id": edge.id,
              onClick: () => emit("edgeClick", { edge }),
            },
            `${edge.source}->${edge.target}:${edge.kind}`,
          ),
        ),
      ]);
  },
});

const sample: PipelineDocument = {
  Start: { recognition: "TemplateMatch", action: "Click", next: ["Step2"], on_error: ["Retry"] },
  Step2: { recognition: { type: "OCR", param: {} }, action: "InputText", next: [{ name: "End" }] },
  Retry: { recognition: "DirectHit", action: "DoNothing", next: ["Step2"] },
  End: { recognition: "DirectHit", action: "DoNothing" },
};

function mountEditor(
  document: PipelineDocument,
  extra: { issues?: ValidationIssue[]; positions?: Record<string, NodePosition> } = {},
) {
  return mount(GraphEditor, {
    props: { document, ...extra },
    global: { stubs: { VueFlow: VueFlowStub } },
  });
}

function flowStub(wrapper: VueWrapper) {
  return wrapper.findComponent({ name: "VueFlowStub" });
}

function nodeNames(wrapper: VueWrapper) {
  return wrapper.findAll(".node").map((item) => item.text());
}

function edgeTexts(wrapper: VueWrapper) {
  return wrapper.findAll(".edge").map((item) => item.text());
}

describe("GraphEditor 渲染", () => {
  it("渲染出全部节点", () => {
    const wrapper = mountEditor(sample);
    expect(wrapper.findAll(".node")).toHaveLength(4);
    expect(nodeNames(wrapper)).toEqual(
      expect.arrayContaining(["Start", "Step2", "Retry", "End"]),
    );
  });

  it("next 与 on_error 各生成一条连线", () => {
    const edges = edgeTexts(mountEditor(sample));
    expect(edges).toContain("Start->Step2:next");
    expect(edges).toContain("Start->Retry:on_error");
    expect(edges).toContain("Retry->Step2:next");
  });

  it("对象形式的后继取 name 作为目标", () => {
    expect(edgeTexts(mountEditor(sample))).toContain("Step2->End:next");
  });

  it("[JumpBack] 渲染为 jumpback 类型的标记节点", () => {
    const wrapper = mountEditor({ Start: { next: ["[JumpBack]"] } });
    const marker = wrapper.findAll(".node").find((item) => item.text() === "[JumpBack]");
    expect(marker?.attributes("data-type")).toBe("jumpback");
    expect(marker?.attributes("data-id")).toBe(JUMPBACK_ID);
    expect(edgeTexts(wrapper)).toContain(`Start->${JUMPBACK_ID}:next`);
  });

  it("空文档显示引导文案，非空时显示操作提示", () => {
    expect(mountEditor({}).text()).toContain("尚未加载节点");
    expect(mountEditor(sample).text()).toContain("拖拽节点");
  });

  it("工具栏显示节点与连线数量", () => {
    // sample 共 4 个节点、4 条连线（Start→Step2、Start→Retry、Retry→Step2、Step2→End）
    expect(mountEditor(sample).text()).toContain("4 节点 · 4 连线");
  });

  it("校验角标传入节点数据", () => {
    const wrapper = mountEditor(sample, {
      issues: [
        { node: "Start", level: "error", field: "recognition.template", message: "缺少 template" },
      ],
    });
    const start = wrapper.findAll(".node").find((item) => item.text() === "Start");
    expect(start?.attributes("data-errors")).toBe("1");
  });

  it("手动摆放的位置优先于自动布局", () => {
    const wrapper = mountEditor(sample, { positions: { Start: { x: 12, y: 34 } } });
    const nodes = flowStub(wrapper).props("nodes") as PipelineGraphNode[];
    expect(nodes.find((node) => node.id === "Start")?.position).toEqual({ x: 12, y: 34 });
  });
});

describe("GraphEditor 交互", () => {
  it("点击节点抛出 select，回跳标记节点不触发", async () => {
    const wrapper = mountEditor({ Start: { next: ["[JumpBack]"] } });
    await wrapper.findAll(".node")[0].trigger("click");
    expect(wrapper.emitted("select")).toEqual([["Start"]]);

    await wrapper.findAll(".node")[1].trigger("click");
    expect(wrapper.emitted("select")).toHaveLength(1);
  });

  it("拖拽节点结束后抛出 move", () => {
    const wrapper = mountEditor(sample);
    flowStub(wrapper).vm.$emit("nodeDragStop", { node: { id: "Start", position: { x: 5, y: 6 } } });
    expect(wrapper.emitted("move")).toEqual([[{ name: "Start", position: { x: 5, y: 6 } }]]);
  });

  it("拖拽回跳标记节点不抛出 move", () => {
    const wrapper = mountEditor({ Start: { next: ["[JumpBack]"] } });
    flowStub(wrapper).vm.$emit("nodeDragStop", {
      node: { id: JUMPBACK_ID, position: { x: 5, y: 6 } },
    });
    expect(wrapper.emitted("move")).toBeUndefined();
  });

  it("右侧出口连线得到 next，下方出口得到 on_error", () => {
    const wrapper = mountEditor(sample);
    flowStub(wrapper).vm.$emit("connect", {
      source: "Start",
      target: "End",
      sourceHandle: "next",
    });
    flowStub(wrapper).vm.$emit("connect", {
      source: "Start",
      target: "End",
      sourceHandle: "on_error",
    });

    expect(wrapper.emitted("connect")).toEqual([
      [{ source: "Start", target: "End", kind: "next" }],
      [{ source: "Start", target: "End", kind: "on_error" }],
    ]);
  });

  it("缺少端点的连线被忽略", () => {
    const wrapper = mountEditor(sample);
    flowStub(wrapper).vm.$emit("connect", { source: "Start", target: null, sourceHandle: "next" });
    expect(wrapper.emitted("connect")).toBeUndefined();
  });

  it("点击连线后可用按钮删除，并抛出 disconnect", async () => {
    const wrapper = mountEditor(sample);
    const button = wrapper.findAll("button.tool")[0];
    expect(button.attributes("disabled")).toBeDefined();

    await wrapper.findAll(".edge")[0].trigger("click");
    expect(button.attributes("disabled")).toBeUndefined();

    await button.trigger("click");
    expect(wrapper.emitted("disconnect")).toEqual([
      [{ source: "Start", target: "Step2", kind: "next" }],
    ]);
    // 删除后按钮回到禁用态
    expect(wrapper.findAll("button.tool")[0].attributes("disabled")).toBeDefined();
  });

  it("点击节点会取消连线选中，避免误删", async () => {
    const wrapper = mountEditor(sample);
    await wrapper.findAll(".edge")[0].trigger("click");
    await wrapper.findAll(".node")[0].trigger("click");
    await wrapper.findAll("button.tool")[0].trigger("click");
    expect(wrapper.emitted("disconnect")).toBeUndefined();
  });
});

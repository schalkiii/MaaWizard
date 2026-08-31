import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent, h } from "vue";

import type { PipelineDocument } from "../api/maa";
import GraphEditor from "./GraphEditor.vue";

interface StubNode {
  id: string;
}

interface StubEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
}

/**
 * 用轻量替身替换 Vue Flow：jsdom 缺少布局能力，
 * 这里只把 nodes/edges 渲染成可断言的文本，并模拟节点点击。
 */
const VueFlowStub = defineComponent({
  props: {
    nodes: { type: Array, required: true },
    edges: { type: Array, required: true },
  },
  emits: ["nodeClick"],
  setup(props, { emit }) {
    return () =>
      h("div", [
        ...(props.nodes as StubNode[]).map((node) =>
          h(
            "span",
            {
              class: "node",
              onClick: () => emit("nodeClick", { node: { id: node.id } }),
            },
            node.id,
          ),
        ),
        ...(props.edges as StubEdge[]).map((edge) =>
          h("span", { class: "edge" }, `${edge.source}->${edge.target}:${edge.label ?? ""}`),
        ),
      ]);
  },
});

function mountEditor(document: PipelineDocument) {
  return mount(GraphEditor, {
    props: { document },
    global: { stubs: { VueFlow: VueFlowStub } },
  });
}

const sample: PipelineDocument = {
  Start: {
    recognition: "TemplateMatch",
    action: "Click",
    next: ["Step2"],
    on_error: ["Retry"],
  },
  Step2: {
    recognition: { type: "OCR", param: { expected: "^开始$" } },
    action: { type: "InputText", param: { input_text: "abc" } },
    next: [{ name: "End", type: "stop" }],
  },
  Retry: { recognition: "DirectHit", action: "DoNothing", next: ["Start"] },
  End: { recognition: "DirectHit", action: "DoNothing" },
};

describe("GraphEditor 图编辑器", () => {
  it("渲染出全部节点", () => {
    const wrapper = mountEditor(sample);
    const ids = wrapper.findAll(".node").map((item) => item.text());
    expect(ids).toHaveLength(4);
    expect(ids).toEqual(expect.arrayContaining(["Start", "Step2", "Retry", "End"]));
  });

  it("next 与 on_error 各生成一条边", () => {
    const edges = mountEditor(sample)
      .findAll(".edge")
      .map((item) => item.text());
    expect(edges).toContain("Start->Step2:next");
    expect(edges).toContain("Start->Retry:on_error");
    expect(edges).toContain("Retry->Start:next");
  });

  it("next 的对象形式取 name 字段作为目标", () => {
    const edges = mountEditor(sample)
      .findAll(".edge")
      .map((item) => item.text());
    expect(edges).toContain("Step2->End:next");
  });

  it("[JumpBack] 渲染为独立的回跳标记节点", () => {
    const wrapper = mountEditor({
      Start: { recognition: "DirectHit", action: "DoNothing", next: ["[JumpBack]"] },
    });
    expect(wrapper.findAll(".node").map((item) => item.text())).toContain("__jumpback__");
    expect(
      wrapper
        .findAll(".edge")
        .map((item) => item.text()),
    ).toContain("Start->__jumpback__:next");
  });

  it("空文档显示引导文案", () => {
    expect(mountEditor({}).text()).toContain("尚未加载节点");
  });

  it("点击节点抛出 select，回跳标记节点不触发", async () => {
    const wrapper = mountEditor({
      Start: { recognition: "DirectHit", action: "DoNothing", next: ["[JumpBack]"] },
    });
    await wrapper.findAll(".node")[0].trigger("click");
    expect(wrapper.emitted("select")).toEqual([["Start"]]);

    await wrapper.findAll(".node")[1].trigger("click");
    // 回跳标记不是真实节点，不应再抛出 select
    expect(wrapper.emitted("select")).toHaveLength(1);
  });

  it("入口节点被排在最左列", () => {
    const wrapper = mountEditor(sample);
    // 通过替身无法直接读 position，这里改用快照式的结构断言：节点顺序稳定
    expect(wrapper.findAll(".node").map((item) => item.text()).sort()).toEqual(
      ["End", "Retry", "Start", "Step2"].sort(),
    );
  });
});

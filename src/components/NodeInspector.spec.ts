import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import type { PipelineNodeData } from "../api/maa";
import NodeInspector from "./NodeInspector.vue";

const templateNode: PipelineNodeData = {
  recognition: { type: "TemplateMatch", param: { template: "start.png" } },
  action: { type: "Click", param: {} },
  next: ["Step2"],
  on_error: [],
  timeout: 8000,
};

function mountInspector(node: PipelineNodeData, controller = "win32") {
  return mount(NodeInspector, { props: { name: "Start", node, controller } });
}

function selectValues(wrapper: ReturnType<typeof mountInspector>) {
  return wrapper.findAll("select").map((item) => (item.element as HTMLSelectElement).value);
}

describe("NodeInspector 节点属性面板", () => {
  it("从 V2 结构载入识别/动作类型与参数", () => {
    const wrapper = mountInspector(templateNode);
    expect(selectValues(wrapper)).toEqual(["TemplateMatch", "Click"]);
    expect(wrapper.findAll("textarea")[0].element.value).toContain("start.png");
    // 展示了 TemplateMatch 的效果说明
    expect(wrapper.text()).toContain("找图");
  });

  it("从 V1 字符串结构也能正确载入", () => {
    const wrapper = mountInspector({ recognition: "DirectHit", action: "DoNothing" });
    expect(selectValues(wrapper)).toEqual(["DirectHit", "DoNothing"]);
  });

  it("切换识别类型后展示对应的效果与参数说明", async () => {
    const wrapper = mountInspector(templateNode);
    await wrapper.findAll("select")[0].setValue("OCR");
    expect(wrapper.text()).toContain("识别画面中的文字");
    expect(wrapper.text()).toContain("expected");
  });

  it("切换动作类型后展示对应的效果说明", async () => {
    const wrapper = mountInspector(templateNode);
    await wrapper.findAll("select")[1].setValue("Swipe");
    expect(wrapper.text()).toContain("从起点滑动到终点");
  });

  it("非 Adb 控制器下选中 Shell 给出不支持提示", () => {
    const wrapper = mountInspector({ recognition: "DirectHit", action: "Shell" }, "win32");
    expect(wrapper.text()).toContain("Shell 仅支持 Adb 控制器");
  });

  it("Adb 控制器下选中 Shell 不给出该提示", () => {
    const wrapper = mountInspector({ recognition: "DirectHit", action: "Shell" }, "adb");
    expect(wrapper.text()).not.toContain("Shell 仅支持 Adb 控制器");
  });

  it("参数 JSON 非法时提示错误且不触发保存", async () => {
    const wrapper = mountInspector(templateNode);
    await wrapper.findAll("textarea")[0].setValue("{ 非法 json");
    await wrapper.find("button.primary").trigger("click");
    expect(wrapper.text()).toContain("参数 JSON 解析失败");
    expect(wrapper.emitted("save")).toBeUndefined();
  });

  it("保存时输出 V2 结构并过滤空白后继", async () => {
    const wrapper = mountInspector({
      recognition: { type: "OCR", param: { expected: "^开始$" } },
      action: { type: "Click", param: {} },
      next: ["Step2", "  "],
      on_error: ["", "Retry"],
      pre_delay: 300,
    });
    await wrapper.find("button.primary").trigger("click");

    const events = wrapper.emitted("save");
    expect(events).toBeDefined();
    const payload = events![0][0] as { name: string; node: PipelineNodeData };
    expect(payload.name).toBe("Start");
    expect(payload.node.recognition).toEqual({
      type: "OCR",
      param: { expected: "^开始$" },
    });
    expect(payload.node.action).toEqual({ type: "Click", param: {} });
    expect(payload.node.next).toEqual(["Step2"]);
    expect(payload.node.on_error).toEqual(["Retry"]);
    expect(payload.node.pre_delay).toBe(300);
  });

  it("未设置的公共字段不会写入节点", async () => {
    const wrapper = mountInspector({
      recognition: "DirectHit",
      action: "DoNothing",
    });
    await wrapper.find("button.primary").trigger("click");
    const payload = wrapper.emitted("save")![0][0] as { node: PipelineNodeData };
    expect(payload.node.timeout).toBeUndefined();
    expect(payload.node.inverse).toBeUndefined();
  });

  it("node 属性变化后表单同步刷新", async () => {
    const wrapper = mountInspector(templateNode);
    await wrapper.setProps({
      node: { recognition: "ColorMatch", action: "StopTask" } satisfies PipelineNodeData,
    });
    expect(selectValues(wrapper)).toEqual(["ColorMatch", "StopTask"]);
  });
});

import { flushPromises, mount, type DOMWrapper } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import RoiCapture from "./RoiCapture.vue";

// vi.mock 会被提升到 import 之前，因此用 hoisted 声明可被引用的桩函数
const { captureScreenshot, captureGrabTemplate } = vi.hoisted(() => ({
  captureScreenshot: vi.fn(),
  captureGrabTemplate: vi.fn(),
}));

vi.mock("../api/maa", () => ({
  captureScreenshot: (output: string) => captureScreenshot(output),
  captureGrabTemplate: (
    x: number,
    y: number,
    width: number,
    height: number,
    resourceDir: string,
  ) => captureGrabTemplate(x, y, width, height, resourceDir),
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

const RESOURCE_DIR = "C:/res";

beforeEach(() => {
  captureScreenshot.mockReset().mockResolvedValue(`${RESOURCE_DIR}/.screenshot.png`);
  captureGrabTemplate
    .mockReset()
    .mockResolvedValue({ file: "template.png", roi: [10, 10, 50, 30] });
});

/** 截屏成功后舞台才会渲染，返回已就绪的组件与舞台元素 */
async function mountWithScreenshot() {
  const wrapper = mount(RoiCapture, { props: { resourceDir: RESOURCE_DIR } });
  await wrapper.findAll("button")[0].trigger("click");
  await flushPromises();
  return { wrapper, stage: wrapper.find(".stage") };
}

async function drag(
  stage: DOMWrapper<Element>,
  from: [number, number],
  to: [number, number],
) {
  await stage.trigger("mousedown", { clientX: from[0], clientY: from[1] });
  await stage.trigger("mousemove", { clientX: to[0], clientY: to[1] });
  await stage.trigger("mouseup", {});
}

describe("RoiCapture ROI 框选与模板抓取", () => {
  it("未截图时显示引导文案", () => {
    const wrapper = mount(RoiCapture, { props: { resourceDir: RESOURCE_DIR } });
    expect(wrapper.text()).toContain("点击「截取屏幕」后");
    expect(wrapper.find(".stage").exists()).toBe(false);
  });

  it("截图后渲染舞台", async () => {
    const { wrapper, stage } = await mountWithScreenshot();
    expect(stage.exists()).toBe(true);
    expect(wrapper.emitted("log")).toEqual([["已截图：C:/res/.screenshot.png"]]);
  });

  it("拖拽框选后回显尺寸并显示选区", async () => {
    const { wrapper, stage } = await mountWithScreenshot();
    await drag(stage, [10, 10], [60, 40]);
    expect(wrapper.text()).toContain("50×30");
    expect(wrapper.find(".selection").exists()).toBe(true);
  });

  it("反向拖拽得到同样的正向矩形", async () => {
    const { wrapper, stage } = await mountWithScreenshot();
    await drag(stage, [60, 40], [10, 10]);
    expect(wrapper.text()).toContain("50×30");
  });

  it("未框选就抓取时提示且不调用后端", async () => {
    const { wrapper } = await mountWithScreenshot();
    await wrapper.findAll("button")[1].trigger("click");
    await flushPromises();

    expect(captureGrabTemplate).not.toHaveBeenCalled();
    expect(wrapper.emitted("log")).toContainEqual(["请先在截图上拖拽框选一块区域"]);
  });

  it("抓取模板时按原始像素换算并抛出 apply", async () => {
    const { wrapper, stage } = await mountWithScreenshot();
    await drag(stage, [10, 10], [60, 40]);
    await wrapper.findAll("button")[1].trigger("click");
    await flushPromises();

    // jsdom 下 clientWidth 为 0，缩放比回退为 1，坐标即 CSS 坐标
    expect(captureGrabTemplate).toHaveBeenCalledWith(10, 10, 50, 30, RESOURCE_DIR);
    expect(wrapper.emitted("apply")?.[0]).toEqual(["template.png"]);
  });

  it("后端抓取失败时记录日志且不抛出 apply", async () => {
    captureGrabTemplate.mockRejectedValue(new Error("裁剪失败"));
    const { wrapper, stage } = await mountWithScreenshot();
    await drag(stage, [10, 10], [60, 40]);
    await wrapper.findAll("button")[1].trigger("click");
    await flushPromises();

    expect(wrapper.emitted("log")).toContainEqual([
      "抓取模板失败：Error: 裁剪失败",
    ]);
    expect(wrapper.emitted("apply")).toBeUndefined();
  });
});

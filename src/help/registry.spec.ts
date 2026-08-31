import { describe, expect, it } from "vitest";

import {
  ACTION_HELP,
  ACTION_TYPES,
  contextualHints,
  NODE_FIELD_HELP,
  RECOGNITION_HELP,
  RECOGNITION_TYPES,
} from "./registry";

describe("使用指引注册表", () => {
  it("每种识别类型都有说明条目", () => {
    for (const type of RECOGNITION_TYPES) {
      const help = RECOGNITION_HELP[type];
      expect(help, `缺少 ${type} 的说明`).toBeDefined();
      expect(help.effect).toBeTruthy();
      expect(help.scene).toBeTruthy();
    }
  });

  it("每种动作类型都有说明条目", () => {
    for (const type of ACTION_TYPES) {
      const help = ACTION_HELP[type];
      expect(help, `缺少 ${type} 的说明`).toBeDefined();
      expect(help.effect).toBeTruthy();
    }
  });

  it("覆盖了 spec 要求的核心识别与动作", () => {
    expect(RECOGNITION_TYPES).toEqual(
      expect.arrayContaining([
        "DirectHit",
        "TemplateMatch",
        "FeatureMatch",
        "ColorMatch",
        "OCR",
        "And",
        "Or",
        "Custom",
      ]),
    );
    expect(ACTION_TYPES).toEqual(
      expect.arrayContaining(["Click", "Swipe", "InputText", "ClickKey", "Shell", "Command"]),
    );
  });

  it("节点公共字段说明均非空", () => {
    expect(NODE_FIELD_HELP.length).toBeGreaterThan(5);
    for (const field of NODE_FIELD_HELP) {
      expect(field.name).toBeTruthy();
      expect(field.desc).toBeTruthy();
    }
  });
});

describe("contextualHints 上下文提示", () => {
  it("TemplateMatch 提示先框选生成模板", () => {
    const hints = contextualHints("TemplateMatch", "Click", "win32");
    expect(hints.some((hint) => hint.includes("模板"))).toBe(true);
  });

  it("Shell 在非 Adb 控制器下提示不可用，Adb 下不提示", () => {
    expect(
      contextualHints("DirectHit", "Shell", "win32").some((hint) => hint.includes("Shell")),
    ).toBe(true);
    expect(
      contextualHints("DirectHit", "Shell", "adb").some((hint) => hint.includes("Shell")),
    ).toBe(false);
  });

  it("Scroll 在非 Win32 控制器下提示", () => {
    expect(
      contextualHints("DirectHit", "Scroll", "adb").some((hint) => hint.includes("Scroll")),
    ).toBe(true);
    expect(
      contextualHints("DirectHit", "Scroll", "win32").some((hint) => hint.includes("Scroll")),
    ).toBe(false);
  });

  it("OCR 提示 expected 是正则", () => {
    expect(
      contextualHints("OCR", "Click", "adb").some((hint) => hint.includes("正则")),
    ).toBe(true);
  });

  it("无风险组合不产生提示", () => {
    expect(contextualHints("DirectHit", "Click", "win32")).toEqual([]);
  });

  it("多个条件同时命中时合并提示", () => {
    const hints = contextualHints("OCR", "Shell", "win32");
    expect(hints.some((hint) => hint.includes("正则"))).toBe(true);
    expect(hints.some((hint) => hint.includes("Shell"))).toBe(true);
  });
});

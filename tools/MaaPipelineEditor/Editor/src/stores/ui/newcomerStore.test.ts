import { beforeEach, describe, expect, it } from "vitest";
import { useNewcomerStore, isNewcomerPassed } from "./newcomerStore";

describe("newcomerStore.skip", () => {
  beforeEach(() => {
    localStorage.clear();
    useNewcomerStore.setState({ passed: false, modalOpen: true });
  });

  it("标记通过并关闭弹窗，无需答题", () => {
    useNewcomerStore.getState().skip();
    const after = useNewcomerStore.getState();
    expect(after.passed).toBe(true);
    expect(after.modalOpen).toBe(false);
    expect(localStorage.getItem("mpe_newcomer_passed")).toBe("true");
    expect(isNewcomerPassed()).toBe(true);
  });

  it("广播 mpe:newcomer-passed 事件", () => {
    let fired = false;
    window.addEventListener("mpe:newcomer-passed", () => {
      fired = true;
    });
    useNewcomerStore.getState().skip();
    expect(fired).toBe(true);
  });

  it("markPassed 同样写入通过标记", () => {
    useNewcomerStore.getState().markPassed();
    expect(isNewcomerPassed()).toBe(true);
  });
});

/**
 * 组件测试环境准备。
 * jsdom 未实现 ResizeObserver，而 Vue Flow 等依赖会用到，这里补一个空实现。
 */

if (!("ResizeObserver" in globalThis)) {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

export {};

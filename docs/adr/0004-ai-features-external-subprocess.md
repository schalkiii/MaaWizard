# AI 增强能力通过外部子进程调用，不内置 Python 运行时

M6 的 maafw-cli / MaaMCP 均为 Python 程序。决定：阶段 5 通过 `tokio::process` 启动外部 Python 进程（要求用户环境已装 `uv`/Python，或提供一键安装），不将 Python 解释器打包进安装包。

- **原因**：避免安装包体积膨胀与跨平台 Python 分发复杂性；maafw-cli 官方推荐 `uvx` 运行。若无 Python 环境，AI 功能优雅降级（UI 提示安装）。
- **备选**：PyO3 内嵌解释器 / 将 MaaMCP 用 Rust 重写。前者增重，后者重复造轮子，故不采用。
- **状态**：proposed

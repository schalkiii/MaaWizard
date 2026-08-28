# MaaWizard 统一命令入口
# 约定：所有构建/清理动作统一通过 make 目标触发，不在命令行中直接执行 rm

.PHONY: help deps dev build check lint lint-rust lint-ts lint-md test test-rust clean distclean fetch-sdk

help:
	@echo "可用目标："
	@echo "  make deps        安装前端依赖（npm install）"
	@echo "  make check       仅做 Rust 类型/语法检查（cargo check，最快）"
	@echo "  make lint        运行全部 lint（markdown + vue-tsc + cargo clippy）"
	@echo "  make test        运行 Rust 单元测试（cargo test）"
	@echo "  make dev         启动 Tauri 开发模式（前端热更新 + Rust 调试构建）"
	@echo "  make build       生产构建，产出安装包"
	@echo "  make fetch-sdk   下载并解压 MaaFramework 官方运行时到 ./maa-sdk"
	@echo "  make clean       清理构建产物（dist / target / gen）"
	@echo "  make distclean   在 clean 基础上额外清理 node_modules 与 SDK"

deps:
	npm install

# 仅检查 Rust 侧能否编译，不涉及链接，用于快速迭代 M0 运行时封装
check:
	cd src-tauri && cargo check

lint: lint-md lint-ts lint-rust

lint-md:
	npx markdownlint-cli2

lint-ts:
	npx vue-tsc --noEmit

lint-rust:
	cd src-tauri && cargo clippy

test: test-rust

test-rust:
	cd src-tauri && cargo test

deps:
	npm install

# 仅检查 Rust 侧能否编译，不涉及链接，用于快速迭代 M0 运行时封装
check:
	cd src-tauri && cargo check

dev:
	npm run tauri dev

build:
	npm run tauri build

# 下载官方预编译运行时；dynamic 链接模式下编译期不需要，仅运行时需要
fetch-sdk:
	powershell -NoProfile -ExecutionPolicy Bypass -File tools/fetch-maa-sdk.ps1

clean:
	powershell -NoProfile -ExecutionPolicy Bypass -Command \
		"Remove-Item -Recurse -Force dist,src-tauri\\target,src-tauri\\gen -ErrorAction SilentlyContinue"

distclean: clean
	powershell -NoProfile -ExecutionPolicy Bypass -Command \
		"Remove-Item -Recurse -Force node_modules,maa-sdk -ErrorAction SilentlyContinue"

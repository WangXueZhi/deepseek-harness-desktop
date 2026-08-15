# Agent Note：桌面运行时使用目标平台原生依赖

Status: implemented

中文 | [English](2026-08-15-desktop-target-runtime-native-dependencies.md)

## 问题

桌面运行时由构建安装包的 runner 组装，但 Harness 的部分依赖通过可选依赖发布平台专用原生包。因此，在 macOS 上准备的运行时可能带有 Darwin addon，却缺少 Windows Web profile 所需的 Windows addon。打包后的 Node 进程会在本地服务监听端口前退出，而桌面壳层只显示笼统的退出码。

## 决策

`prepare-desktop-runtime.ts` 在部署 dsh 包前向 pnpm 传入请求的目标操作系统和 CPU。运行时完成物化后，脚本校验目标平台原生包；缺少必需 addon 时直接使打包失败。桌面启动过程把 dsh 的标准输出和错误输出写入应用数据目录日志，并在失败状态中包含日志路径和最近的日志行。Windows 进程树清理会隐藏预期的 `taskkill` 输出，并且子进程已退出时不再调用 `taskkill`。

## 后果

桌面产物必须在准备依赖时指定目标平台。宁可让打包失败，也不发布无法启动内置 Node 进程的安装包。用户可以从应用数据目录读取启动失败原因，无需打开终端。

## 验证

打包后的 macOS 运行时能够启动 Web profile 并报告本地 URL。桌面 Rust 测试覆盖现有启动生命周期辅助函数，UI 冒烟测试断言可操作的启动失败提示。Windows 安装包验证仍由 Windows release runner 负责。

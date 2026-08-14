# Agent Note: Desktop installer release workflow

Status: implemented

[English](2026-08-14-desktop-installer-release-workflow.md) | 中文

## 问题

桌面应用会内置对应平台的 Node.js 运行时和已构建的 Harness CLI/Web 文件，因此在一个操作系统上构建的安装包无法证明或提供另一个操作系统的载荷。手工上传的文件可能来自不同提交、遗漏受支持架构，或缺少用于识别实际下载字节的校验和。

## 决策

[`Desktop Release`](../../../../.github/workflows/desktop-release.yml) 工作流是 GitHub Release 安装包的来源。一次手动运行会从同一提交在匹配架构的 GitHub 托管环境上构建 macOS arm64、macOS x64 和 Windows x64。每个作业都会构建 Harness，通过 [`prepare-desktop-runtime.ts`](../../../../scripts/prepare-desktop-runtime.ts) 准备匹配的内置 Node.js 运行时，为对应 Rust 目标运行 Tauri bundler，验证安装包存在，并且只保留可分发的安装包文件。

只有调度者选择 `publish=true` 时，独立的发布作业才会运行。请求的标签必须等于 `tauri.conf.json` 版本生成的 `desktop-v<version>`。该作业下载本次工作流运行的全部安装包，生成 `SHA256SUMS`，并把这些经过保留的精确文件上传到指向工作流提交的 GitHub Release。只有这个作业获得仓库内容写权限。

发布文件属于未签名预览版。代码签名、macOS notarization 和 Windows 签名仍需要独立的发布凭据，构建或上传成功不代表这些步骤已经完成。

## 考虑过的替代方案

**上传本机构建。** macOS 本地上传无法生成原生 Windows 安装包，也不能证明全部文件来自同一提交。工作流会在发布前构建并汇总完整集合。

**通过交叉编译构建 Windows。** Tauri 的 Windows 打包工具链和安装包生成器依赖原生 Windows 环境。工作流使用 `windows-2025`，而不是维护基于 Wine 的发布环境。

**每次推送 master 都发布。** 仓库推送属于开发事件，不应自动生成可供外部下载的二进制文件。手动调度、由精确版本生成的标签和独立的写权限作业让发布成为明确操作。

## 后果

一次工作流运行会生成 macOS arm64、macOS x64、Windows MSI 和 Windows NSIS 候选文件，并将其保留为短期 Actions 产物，或者连同校验和一起发布。任一平台失败都会阻止发布，因此发行版不会静默遗漏 Windows 或某个 macOS 架构。

在配置签名和 notarization 之前，发行版用户必须接受操作系统针对未签名软件的警告。重新运行已经发布的标签会替换 Release 资产，因此维护者必须从目标提交调度工作流，并在调查替换时比较 `SHA256SUMS`。

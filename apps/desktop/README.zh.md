# DeepSeek Harness 桌面版

[English](README.md) | 中文

此 package 包含 DeepSeek Harness Web 应用的 Tauri 壳。`pnpm run desktop:prepare-runtime` 命令会先填充被忽略的 `runtime/<target>/` 目录，再由 `pnpm --filter @deepseek-ai/dsh-desktop run build` 创建安装包。

每个发行 runtime 包含：

- `node/node` 或 `node/node.exe`
- `dsh/`，其中包含 `@deepseek-ai/dsh` 的 production deployment

部署产物会被扁平化并移除符号链接，因此同一载荷可以复制进 Tauri macOS 或 Windows bundle。准备脚本会恢复动态 Web profile 使用的 workspace 包，以及 `pnpm deploy --legacy` 不会放在 package 根目录的 hoisted peer dependencies。

Node.js 二进制文件和已准备的 runtime 不会提交到仓库。使用现有本地 Node 二进制进行测试时可以设置 `DESKTOP_NODE_BIN`；发行构建会为所选目标下载固定版本的 Node.js archive。

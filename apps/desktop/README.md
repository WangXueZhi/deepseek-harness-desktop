# DeepSeek Harness Desktop

English | [中文](README.zh.md)

This package contains the Tauri shell for the DeepSeek Harness Web application. The `pnpm run desktop:prepare-runtime` command populates the ignored `runtime/<target>/` directories before `pnpm --filter @deepseek-ai/dsh-desktop run build` creates an installer.

Each release runtime contains:

- `node/node` or `node/node.exe`
- `dsh/` with the production deployment of `@deepseek-ai/dsh`

The deployment is flattened and symlink-free so the same payload can be copied into a Tauri macOS or Windows bundle. The preparation script restores workspace packages used by the dynamic Web profile and the hoisted peer dependencies that `pnpm deploy --legacy` does not place at the package root.

Node.js binaries and prepared runtimes are intentionally not committed. Set `DESKTOP_NODE_BIN` when testing with an existing local Node binary; release builds download the pinned Node.js archive for the selected target.

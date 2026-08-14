# Desktop Runtime

This directory is populated by `pnpm run desktop:prepare-runtime`.

Each release target contains:

- `node/node` or `node/node.exe`
- `dsh/` with the production deployment of `@deepseek-ai/dsh`

The deployment is flattened and symlink-free so the same payload can be copied into a Tauri macOS or Windows bundle. The script also restores workspace packages used by the dynamic Web profile and the hoisted peer dependencies that `pnpm deploy --legacy` does not place at the package root.

The Node.js binary is intentionally not committed to the repository. Use the
runtime preparation script before building an installer. Set `DESKTOP_NODE_BIN`
when testing with an existing local Node binary; release builds should download
the pinned Node.js archive for the selected target.

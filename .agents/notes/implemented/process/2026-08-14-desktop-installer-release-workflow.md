# Agent Note: Desktop installer release workflow

Status: implemented

English | [中文](2026-08-14-desktop-installer-release-workflow.zh.md)

## Problem

The desktop application embeds a platform Node.js runtime and the built Harness CLI/Web files, so an installer built on one operating system cannot prove or supply another operating system's payload. Manually uploaded files can come from different commits, omit a supported architecture, or lose the checksums needed to identify the exact downloaded bytes.

## Decision

The [`Desktop Release`](../../../../.github/workflows/desktop-release.yml) workflow is the source of GitHub Release installers. A manual run builds macOS arm64, macOS x64, and Windows x64 from one commit on matching GitHub-hosted architectures. Each job builds Harness, prepares the matching embedded Node.js runtime through [`prepare-desktop-runtime.ts`](../../../../scripts/prepare-desktop-runtime.ts), runs the Tauri bundler for the matching Rust target, verifies that an installer exists, and retains only distributable installer files.

Publication is a separate job that runs only when the dispatcher selects `publish=true`. The requested tag must equal `desktop-v<version>` from `tauri.conf.json`. The job downloads all installers from that workflow run, records `SHA256SUMS`, and uploads the exact retained files to a GitHub Release at the workflow commit. Only that job receives repository content write permission.

The published files are unsigned previews. Code signing, macOS notarization, and Windows signing remain separate release credentials and are not implied by a successful build or upload.

## Alternatives considered

**Upload local builds.** A local upload cannot produce native Windows installers from macOS and does not prove that all files came from one commit. The workflow builds and aggregates the complete set before publication.

**Build Windows through cross-compilation.** Tauri's Windows packaging toolchain and installer generators are native Windows dependencies. The workflow uses `windows-2025` instead of maintaining a Wine-based release environment.

**Publish on every master push.** Repository pushes are development events and must not create externally downloadable binaries. Manual dispatch, an exact version-derived tag, and a separate write-enabled job make publication explicit.

## Consequences

A single workflow run produces macOS arm64, macOS x64, Windows MSI, and Windows NSIS candidates and either retains them as short-lived Actions artifacts or publishes them together with checksums. A failed platform blocks publication, so a release cannot silently omit Windows or one macOS architecture.

Release users must accept operating-system warnings for unsigned software until signing and notarization are configured. Re-running a published tag replaces release assets with files from the new run, so maintainers must dispatch the workflow from the intended commit and compare `SHA256SUMS` when investigating a replacement.

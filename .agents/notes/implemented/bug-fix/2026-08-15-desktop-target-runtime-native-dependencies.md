# Agent Note: Target-specific native dependencies in desktop runtimes

Status: implemented

English | [中文](2026-08-15-desktop-target-runtime-native-dependencies.zh.md)

## Problem

The desktop runtime is assembled on the runner that prepares the installer, but several Harness dependencies publish platform-specific optional packages. A runtime prepared on macOS can therefore contain Darwin addons while omitting the Windows addons required by the Windows web profile. The packaged Node process exits before the local server binds, while the desktop shell only reports a generic exit code.

## Decision

`prepare-desktop-runtime.ts` passes the requested target operating system and CPU to pnpm's supported-architecture configuration before deploying the dsh package. It validates the target's native packages after materialization and fails the packaging step when a required addon is absent. Desktop startup writes both dsh streams to the application data log and includes the log path and recent lines in the failure status. Windows process-tree cleanup suppresses expected `taskkill` output and avoids invoking it after the child has already exited.

## Consequences

Desktop artifacts must be prepared with dependencies installed for the target platform. A packaging failure is preferred to publishing an installer whose embedded Node process cannot boot. Users can diagnose a startup failure from the application data directory without opening a terminal.

## Verification

The packaged macOS runtime boots the web profile and reports its local URL. The desktop Rust tests cover the existing startup lifecycle helpers, and the UI smoke test asserts the actionable startup-failure message. Windows installer verification remains owned by the Windows release runner.

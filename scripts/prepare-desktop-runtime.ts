import { cp, lstat, mkdir, readFile, readdir, realpath, rename, rm, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { execFileSync } from 'node:child_process'
import { tmpdir } from 'node:os'

const root = resolve(import.meta.dirname, '..')
const runtimeRoot = join(root, 'apps/desktop/runtime')
const stagingParent = join(root, '.desktop-staging')
const nodeVersion = process.env.DESKTOP_NODE_VERSION ?? '24.14.0'
const target = process.argv.slice(2).find(argument => argument !== '--') ?? detectTarget()
const nodeTarget = targetNodeTarget(target)
const nodeArchiveName = target.startsWith('windows-') ? 'node.zip' : 'node.tar.gz'
const targetRoot = join(runtimeRoot, target)
const dshRoot = join(targetRoot, 'dsh')
const stagingRelative = join('.desktop-staging', `dsh-desktop-deploy-${target}`)
const stagingRoot = join(root, stagingRelative)
const runtimePeerSource = join(root, 'python/sdk-runtime/node_modules')
const workspacePackageRoots = ['apps/cli', 'packages', 'vendor']

function detectTarget(): string {
  if (process.platform === 'darwin' && process.arch === 'arm64') return 'darwin-arm64'
  if (process.platform === 'darwin' && process.arch === 'x64') return 'darwin-x64'
  if (process.platform === 'win32' && process.arch === 'x64') return 'windows-x64'
  throw new Error(`unsupported desktop runtime target: ${process.platform}-${process.arch}`)
}

function targetNodeTarget(value: string): string {
  const map: Record<string, string> = {
    'darwin-arm64': 'darwin-arm64',
    'darwin-x64': 'darwin-x64',
    'windows-x64': 'win-x64',
  }
  const resolvedTarget = map[value]
  if (!resolvedTarget) throw new Error(`unsupported desktop runtime target: ${value}`)
  return resolvedTarget
}

function nodeArchiveUrl(): string {
  const suffix = target.startsWith('windows-') ? 'win-x64.zip' : `${nodeTarget}.tar.gz`
  return `https://nodejs.org/dist/v${nodeVersion}/node-v${nodeVersion}-${suffix}`
}

function nodeFilename(): string {
  return target.startsWith('windows-') ? 'node.exe' : 'node'
}

function extractionSuffix(): string {
  return target.startsWith('windows-') ? 'win-x64' : nodeTarget
}

function pnpmCommand(): { command: string; prefix: string[] } {
  const pnpmExecPath = process.env.PNPM_EXECUTABLE ?? process.env.npm_execpath
  const isNodeScript = pnpmExecPath?.endsWith('.cjs') || pnpmExecPath?.endsWith('.mjs')
  return {
    command: isNodeScript ? process.execPath : pnpmExecPath ?? 'pnpm',
    prefix: isNodeScript && pnpmExecPath ? [pnpmExecPath] : [],
  }
}

async function downloadNode(): Promise<void> {
  const archivePath = join(tmpdir(), `dsh-node-${target}-${nodeVersion}-${nodeArchiveName}`)
  const extractionDir = join(tmpdir(), `dsh-node-${target}-${nodeVersion}`)
  const response = await fetch(nodeArchiveUrl())
  if (!response.ok || !response.body) {
    throw new Error(`failed to download Node.js: ${response.status} ${response.statusText}`)
  }
  await writeFile(archivePath, Buffer.from(await response.arrayBuffer()))
  await rm(extractionDir, { recursive: true, force: true })
  await mkdir(extractionDir, { recursive: true })
  try {
    const extractionArgs = target.startsWith('windows-')
      ? ['-xf', archivePath, '-C', extractionDir]
      : ['-xzf', archivePath, '-C', extractionDir]
    execFileSync('tar', extractionArgs)
    const extractedRoot = join(extractionDir, `node-v${nodeVersion}-${extractionSuffix()}`)
    const nodeSource = join(extractedRoot, target.startsWith('windows-') ? 'node.exe' : 'bin/node')
    await mkdir(join(targetRoot, 'node'), { recursive: true })
    await cp(nodeSource, join(targetRoot, 'node', nodeFilename()), { force: true })
  } finally {
    await rm(archivePath, { force: true })
    await rm(extractionDir, { recursive: true, force: true })
  }
}

async function deployDsh(): Promise<void> {
  if (!existsSync(join(root, 'apps/cli/lib/bin.js'))) {
    throw new Error('dsh is not built; run pnpm run build before preparing the desktop runtime')
  }
  const workspacePackages = await collectWorkspacePackages()
  await rm(stagingRoot, { recursive: true, force: true })
  await rm(dshRoot, { recursive: true, force: true })
  await mkdir(stagingParent, { recursive: true })
  const { command, prefix } = pnpmCommand()
  execFileSync(command, [
    ...prefix,
    '--config.manage-package-manager-versions=false',
    '--filter',
    '@deepseek-ai/dsh',
    'deploy',
    '--prod',
    '--legacy',
    stagingRelative,
  ], {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, CI: 'true' },
  })
  await cleanWorkspaceDeployResidue(workspacePackages)
  await restoreWorkspacePackages(workspacePackages)
  await restoreMissingDirectDependencies(workspacePackages)
  await restoreVirtualStoreHoists(workspacePackages)
  await materializeTopLevelDependencies()
  await mkdir(dirname(dshRoot), { recursive: true })
  await cp(stagingRoot, dshRoot, {
    recursive: true,
    dereference: false,
    filter: path => !isVirtualStorePath(path),
  })
  await rm(stagingRoot, { recursive: true, force: true })
}

async function cleanWorkspaceDeployResidue(workspacePackages: Map<string, string>): Promise<void> {
  for (const packageRoot of new Set(workspacePackages.values())) {
    await rm(join(packageRoot, '.desktop-staging'), { recursive: true, force: true })
  }
}

function isVirtualStorePath(path: string): boolean {
  const normalized = path.split(sep).join('/')
  return normalized.includes('node_modules/.pnpm')
}

async function readStagedDependencies(): Promise<string[]> {
  const manifest = JSON.parse(await readFile(join(stagingRoot, 'package.json'), 'utf8')) as {
    dependencies?: Record<string, string>
  }
  return Object.keys(manifest.dependencies ?? {}).sort()
}

async function restoreMissingDirectDependencies(workspacePackages: Map<string, string>): Promise<void> {
  for (const dependency of await readStagedDependencies()) {
    const destination = join(stagingRoot, 'node_modules', dependency)
    if (existsSync(destination)) continue
    const source = workspacePackages.get(dependency) ?? join(runtimePeerSource, dependency)
    if (!existsSync(source)) {
      throw new Error(`deployed dependency ${dependency} is missing from ${destination} and ${source}`)
    }
    await copyPackage(source, destination)
  }
}

async function collectWorkspacePackages(): Promise<Map<string, string>> {
  const packages = new Map<string, string>()
  for (const packageRoot of workspacePackageRoots) {
    await collectWorkspacePackagesFrom(join(root, packageRoot), packages)
  }
  return packages
}

async function collectWorkspacePackagesFrom(directory: string, packages: Map<string, string>): Promise<void> {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name === 'dist') continue
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      await collectWorkspacePackagesFrom(path, packages)
      continue
    }
    if (entry.name !== 'package.json') continue
    const manifest = JSON.parse(await readFile(path, 'utf8')) as { name?: unknown }
    if (typeof manifest.name !== 'string' || !manifest.name.startsWith('@deepseek-ai/')) continue
    packages.set(manifest.name, dirname(path))
  }
}

async function restoreWorkspacePackages(workspacePackages: Map<string, string>): Promise<void> {
  for (const [name, source] of workspacePackages) {
    const destination = join(stagingRoot, 'node_modules', name)
    await copyPackage(source, destination)
  }
}

async function restoreVirtualStoreHoists(workspacePackages: Map<string, string>): Promise<void> {
  const hoistRoot = join(stagingRoot, 'node_modules', '.pnpm', 'node_modules')
  for (const entry of await readdir(hoistRoot, { withFileTypes: true })) {
    if (entry.name === '.bin') continue
    const source = join(hoistRoot, entry.name)
    if (entry.name.startsWith('@') && entry.isDirectory()) {
      for (const scoped of await readdir(source, { withFileTypes: true })) {
        const dependencySource = join(source, scoped.name)
        const destination = join(stagingRoot, 'node_modules', entry.name, scoped.name)
        if (existsSync(destination)) continue
        if (await isExcludedWorkspaceHoist(dependencySource, workspacePackages)) continue
        await copyPackage(dependencySource, destination)
      }
      continue
    }
    const destination = join(stagingRoot, 'node_modules', entry.name)
    if (existsSync(destination)) continue
    if (await isExcludedWorkspaceHoist(source, workspacePackages)) continue
    await copyPackage(source, destination)
  }
}

async function isExcludedWorkspaceHoist(source: string, workspacePackages: Map<string, string>): Promise<boolean> {
  const realSource = await realpath(source)
  if (!realSource.startsWith(root + sep) || isVirtualStorePath(realSource)) return false
  const manifestPath = join(realSource, 'package.json')
  if (!existsSync(manifestPath)) return false
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as { name?: unknown }
  return typeof manifest.name === 'string' && !workspacePackages.has(manifest.name)
}

async function materializeTopLevelDependencies(): Promise<void> {
  const nodeModules = join(stagingRoot, 'node_modules')
  for (const entry of await readdir(nodeModules, { withFileTypes: true })) {
    if (entry.name === '.pnpm' || entry.name === '.modules.yaml' || entry.name === '.bin') continue
    const path = join(nodeModules, entry.name)
    if (entry.name.startsWith('@') && entry.isDirectory()) {
      for (const scoped of await readdir(path, { withFileTypes: true })) {
        const dependency = join(path, scoped.name)
        if ((await lstat(dependency)).isSymbolicLink()) {
          await materializePackage(dependency)
        }
      }
      continue
    }
    if ((await lstat(path)).isSymbolicLink()) {
      await materializePackage(path)
    }
  }
}

async function materializePackage(path: string): Promise<void> {
  const temporary = `${path}.desktop-materialized`
  await rm(temporary, { recursive: true, force: true })
  await copyPackage(path, temporary)
  await rm(path, { recursive: true, force: true })
  await rename(temporary, path)
}

async function copyPackage(source: string, destination: string): Promise<void> {
  await mkdir(dirname(destination), { recursive: true })
  const realSource = await realpath(source)
  await rm(destination, { recursive: true, force: true })
  const nestedNodeModules = join(realSource, 'node_modules')
  await cp(realSource, destination, {
    recursive: true,
    dereference: false,
    filter: path => path !== nestedNodeModules && !path.startsWith(nestedNodeModules + sep),
  })
}

async function main(): Promise<void> {
  try {
    await deployDsh()
    if (process.env.DESKTOP_NODE_BIN) {
      await mkdir(join(targetRoot, 'node'), { recursive: true })
      await cp(resolve(process.env.DESKTOP_NODE_BIN), join(targetRoot, 'node', nodeFilename()), { force: true })
    } else {
      await downloadNode()
    }
    console.log(`Prepared DeepSeek Harness desktop runtime: ${target}`)
  } finally {
    await rm(stagingRoot, { recursive: true, force: true })
  }
}

await main()

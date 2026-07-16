import { spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');

const components = [
  {
    dir: 'native/winbridge',
    build: ['dotnet', 'build', '-c', 'Release', '--nologo'],
    outDir: 'bin/Release/net48',
    artifacts: ['topisland-winbridge.exe', 'topisland-winbridge.exe.config'],
    toolchainHint: '.NET SDK(dotnet.microsoft.com)',
  },
  {
    dir: 'native/wxkey',
    build: ['cargo', 'build', '--release'],
    appendProjectDir: false, // cargo 在项目目录内执行，不接受目录位置参数
    outDir: 'target/release',
    artifacts: ['ti-wxkey.exe'],
    toolchainHint: 'Rust(rustup.rs)',
  },
];

/** 工具在 PATH 之外的常见安装位置：刚装好还没重开终端时 PATH 未刷新，据此兜底 */
const TOOL_FALLBACKS = {
  cargo: () => {
    const home = process.env.CARGO_HOME || path.join(os.homedir(), '.cargo');
    return [path.join(home, 'bin', 'cargo.exe')];
  },
  dotnet: () =>
    [process.env.ProgramFiles, process.env.ProgramW6432, 'C:\\Program Files']
      .filter(Boolean)
      .map((p) => path.join(p, 'dotnet', 'dotnet.exe')),
};

/** 解析工具可执行路径：先试 PATH，再查常见安装位置；都没有返回 null */
function resolveTool(cmd) {
  if (!spawnSync(cmd, ['--version'], { stdio: 'ignore' }).error) return cmd;
  for (const cand of TOOL_FALLBACKS[cmd]?.() ?? []) {
    if (fs.existsSync(cand)) return cand;
  }
  return null;
}

/** 源码文件的最新 mtime（跳过构建目录） */
function newestSourceMtime(dir) {
  let newest = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'bin' || entry.name === 'obj' || entry.name === 'target') continue;
    const full = path.join(dir, entry.name);
    newest = Math.max(newest, entry.isDirectory() ? newestSourceMtime(full) : fs.statSync(full).mtimeMs);
  }
  return newest;
}

function artifactPath(comp) {
  return path.join(root, 'resources', comp.artifacts[0]);
}

function needsBuild(comp, force) {
  if (force) return true;
  const target = artifactPath(comp);
  if (!fs.existsSync(target)) return true;
  return fs.statSync(target).mtimeMs < newestSourceMtime(path.join(root, comp.dir));
}

/**
 * 覆盖目标文件；目标是正在运行的 exe 时，Windows 不允许覆盖但允许重命名——
 * 挪成 .old 再放新文件，运行中进程继续用旧镜像，重启后生效。.old 下次构建清理。
 */
function replaceFile(src, dst) {
  const old = dst + '.old';
  try {
    fs.rmSync(old, { force: true });
  } catch {
    /* 仍被占用，留待下次 */
  }
  try {
    fs.copyFileSync(src, dst);
    return;
  } catch (e) {
    if (e.code !== 'EBUSY' && e.code !== 'EPERM') throw e;
  }
  fs.renameSync(dst, old);
  fs.copyFileSync(src, dst);
  console.warn(`[native] ${path.basename(dst)} 正在运行，已换名替换（进程重启后生效）`);
}

function buildComponent(comp, tool) {
  const projectDir = path.join(root, comp.dir);
  // dotnet 接受项目目录作位置参数；cargo 等需在项目目录内执行（appendProjectDir: false）。
  const [, ...args] = comp.build;
  const spawnOpts = { stdio: 'inherit' };
  if (comp.appendProjectDir === false) spawnOpts.cwd = projectDir;
  else args.push(projectDir);
  const r = spawnSync(tool, args, spawnOpts);
  if (r.error || r.status !== 0) {
    if (fs.existsSync(artifactPath(comp))) {
      console.warn(`[native] ${comp.dir} 构建失败，沿用已有 resources/${comp.artifacts[0]}（可能是旧版）`);
      return;
    }
    throw new Error(`[native] ${comp.dir} 构建失败，需要 ${comp.toolchainHint}`);
  }
  for (const file of comp.artifacts) {
    const src = path.join(projectDir, comp.outDir, file);
    if (!fs.existsSync(src)) continue;
    replaceFile(src, path.join(root, 'resources', file));
    console.log(`[native] resources/${file} 已更新`);
  }
}

/** 保证原生组件产物存在且不旧于源码；force 无条件重建 */
export function ensureNative({ force = false } = {}) {
  const pending = components.filter((c) => needsBuild(c, force));
  if (pending.length === 0) return;

  // 预检工具链：一次把缺的全部列出来（比逐个失败更好排查），并对 PATH 未刷新兜底
  const resolved = new Map();
  const missing = [];
  for (const comp of pending) {
    const tool = resolveTool(comp.build[0]);
    if (tool) resolved.set(comp, tool);
    else if (fs.existsSync(artifactPath(comp)))
      console.warn(`[native] 未找到 ${comp.build[0]}，沿用已有 resources/${comp.artifacts[0]}`);
    else missing.push(comp);
  }
  if (missing.length) {
    const lines = missing.map((c) => `  · ${c.toolchainHint}  —— 构建 ${c.dir}`);
    throw new Error(
      `Native Build 缺少以下工具链：\n${lines.join('\n')}\n\n` +
        `装好后请「重新打开终端/IDE」再构建。`
    );
  }

  for (const comp of pending) {
    const tool = resolved.get(comp);
    if (tool) buildComponent(comp, tool);
  }
}

// 直接执行（pnpm native:build）：无条件重建
if (process.argv[1] && import.meta.url === `file:///${process.argv[1].replace(/\\/g, '/')}`) {
  ensureNative({ force: true });
}

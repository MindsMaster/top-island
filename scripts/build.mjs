import { build as esbuild } from 'esbuild';
import { build as viteBuild } from 'vite';
import * as fs from 'fs';
import { ensureNative } from './build-native.mjs';

const common = {
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node22',
  sourcemap: true,
  // 原生模块不能被 esbuild 打进 bundle，运行时从 node_modules 加载
  external: ['electron', 'better-sqlite3-multiple-ciphers'],
};

export async function buildElectron() {
  // 原生组件：dev/dist 都经此增量重建到 resources/
  ensureNative();
  await esbuild({ ...common, entryPoints: ['electron/main/index.ts'], outfile: 'dist/main.js' });
  await esbuild({ ...common, entryPoints: ['electron/preload/index.ts'], outfile: 'dist/preload.js' });
  // 微信解密/读库放到 worker 线程执行，避免大库解密阻塞主进程（鼠标/IPC 卡顿）
  await esbuild({
    ...common,
    entryPoints: ['electron/main/services/wechat/read-worker.ts'],
    outfile: 'dist/read-worker.js',
  });
  // 让 Node 就近把 dist/*.js（尤其 asarUnpack 出来的 read-worker.js）判为 CJS；
  // 否则向上逐层找 package.json，portable 在 %TEMP% 解压时会撞到坏的那个致 worker 崩。
  fs.writeFileSync('dist/package.json', JSON.stringify({ type: 'commonjs' }) + '\n');
}

// 直接执行：构建全部（主进程 + preload + 渲染层）
if (import.meta.url === `file:///${process.argv[1].replace(/\\/g, '/')}`) {
  await buildElectron();
  await viteBuild();
  console.log('build ok');
}

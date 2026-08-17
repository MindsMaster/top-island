import { execFileSync, execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PUBLIC_URL = 'https://repo.azuramc.cc/repository/raw-public/top-island/';

function pad(n) {
  return String(n).padStart(2, '0');
}

function stampNow() {
  const d = new Date();
  return (
    `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}` +
    `${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`
  );
}

function stampVersion(version) {
  if (!version.endsWith('-SNAPSHOT')) return version;
  return `${version.slice(0, -'-SNAPSHOT'.length)}-${stampNow()}-SNAPSHOT`;
}

function gitHash() {
  try {
    return execSync('git rev-parse --short HEAD', { cwd: ROOT, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

function gradleHomes() {
  const homes = [];
  const seen = new Set();
  const add = (p) => {
    if (!p) return;
    const resolved = path.resolve(p);
    const key = resolved.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    homes.push(resolved);
  };
  add(process.env.GRADLE_USER_HOME);
  add(process.env.GRADLE_HOME);
  add(path.join(os.homedir(), '.gradle'));
  return homes;
}

function gradleProp(name) {
  for (const home of gradleHomes()) {
    const file = path.join(home, 'gradle.properties');
    if (!fs.existsSync(file)) continue;
    for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
      const t = line.trim();
      if (!t || t.startsWith('#') || t.startsWith('//')) continue;
      const i = t.indexOf('=');
      if (i < 0) continue;
      if (t.slice(0, i).trim() === name) return t.slice(i + 1).trim();
    }
  }
  return '';
}

function creds() {
  const user = process.env.AZURA_REPO_USERNAME || gradleProp('azuraRepoUsername');
  const password = process.env.AZURA_REPO_PASSWORD || gradleProp('azuraRepoPassword');
  if (!user || !password) {
    throw new Error(
      '缺少仓库凭证。设置 AZURA_REPO_USERNAME / AZURA_REPO_PASSWORD，或在 GRADLE_USER_HOME/gradle.properties 写入 azuraRepoUsername / azuraRepoPassword'
    );
  }
  return { user, password };
}

function run(cmd, args) {
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit' });
}

function authHeader({ user, password }) {
  return 'Basic ' + Buffer.from(`${user}:${password}`).toString('base64');
}

async function upload(repo, destName, filePath, auth) {
  const url = `https://repo.azuramc.cc/repository/${repo}/top-island/${destName}`;
  const body = fs.readFileSync(filePath);
  const res = await fetch(url, {
    method: 'PUT',
    headers: {
      Authorization: authHeader(auth),
      'Content-Type': 'application/octet-stream',
      'Content-Length': String(body.length),
    },
    body,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(`上传失败 ${res.status} ${url}\n${text}`);
  }
  console.log('uploaded', destName, '->', url.replace('https://repo.azuramc.cc/repository/', ''));
}

const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
const isSnapshot = String(pkg.version).endsWith('-SNAPSHOT');
const version = stampVersion(pkg.version);
const channel = isSnapshot ? 'snapshot' : 'latest';
const repo = isSnapshot ? 'raw-snapshots' : 'raw-releases';
const ymlName = isSnapshot ? 'snapshot.yml' : 'latest.yml';
const hash = gitHash();
const setupName = `TopIsland-Setup-${version}.exe`;

console.log(`dist:push ${version} channel=${channel} git=${hash || '-'}`);

const auth = creds();

run(process.execPath, [path.join(ROOT, 'scripts', 'build.mjs')]);

const builderCli = require.resolve('electron-builder/cli.js');
const builderArgs = [
  builderCli,
  '--win',
  '--publish',
  'never',
  `-c.extraMetadata.version=${version}`,
  `-c.publish.channel=${channel}`,
  `-c.publish.provider=generic`,
  `-c.publish.url=${PUBLIC_URL}`,
  `-c.publish.useMultipleRangeRequest=false`,
];
if (hash) builderArgs.push(`-c.extraMetadata.gitHash=${hash}`);
run(process.execPath, builderArgs);

const outDir = path.join(ROOT, 'release');
const setupPath = path.join(outDir, setupName);
const blockmapPath = setupPath + '.blockmap';
const ymlPath = path.join(outDir, ymlName);
for (const f of [setupPath, blockmapPath, ymlPath]) {
  if (!fs.existsSync(f)) throw new Error(`缺少产物: ${f}`);
}

await upload(repo, setupName, setupPath, auth);
await upload(repo, setupName + '.blockmap', blockmapPath, auth);
await upload(repo, ymlName, ymlPath, auth);
console.log('dist:push ok', `${PUBLIC_URL}${ymlName}`);

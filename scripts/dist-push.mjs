import { execFileSync, execSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

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

function gradleProp(name) {
  const file = path.join(os.homedir(), '.gradle', 'gradle.properties');
  if (!fs.existsSync(file)) return '';
  for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith('#') || t.startsWith('//')) continue;
    const i = t.indexOf('=');
    if (i < 0) continue;
    if (t.slice(0, i).trim() === name) return t.slice(i + 1).trim();
  }
  return '';
}

function creds() {
  const user = process.env.AZURA_REPO_USERNAME || gradleProp('azuraRepoUsername');
  const password = process.env.AZURA_REPO_PASSWORD || gradleProp('azuraRepoPassword');
  if (!user || !password) {
    throw new Error(
      '缺少仓库凭证。设置 AZURA_REPO_USERNAME / AZURA_REPO_PASSWORD，或在 ~/.gradle/gradle.properties 写入 azuraRepoUsername / azuraRepoPassword'
    );
  }
  return { user, password };
}

function run(cmd, args) {
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit', shell: process.platform === 'win32' });
}

async function upload(repo, destName, filePath, { user, password }) {
  const url = `https://repo.azuramc.cc/repository/${repo}/top-island/${destName}`;
  const body = fs.readFileSync(filePath);
  const res = await fetch(url, {
    method: 'PUT',
    headers: {
      Authorization: 'Basic ' + Buffer.from(`${user}:${password}`).toString('base64'),
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

run(process.execPath, [path.join(ROOT, 'scripts', 'build.mjs')]);

const builderArgs = [
  'exec',
  'electron-builder',
  '--win',
  '--publish',
  'never',
  `-c.extraMetadata.version=${version}`,
  `-c.publish.channel=${channel}`,
  `-c.publish.provider=generic`,
  `-c.publish.url=${PUBLIC_URL}`,
];
if (hash) builderArgs.push(`-c.extraMetadata.gitHash=${hash}`);
run('pnpm', builderArgs);

const outDir = path.join(ROOT, 'release');
const setupPath = path.join(outDir, setupName);
const blockmapPath = setupPath + '.blockmap';
const ymlPath = path.join(outDir, ymlName);
for (const f of [setupPath, blockmapPath, ymlPath]) {
  if (!fs.existsSync(f)) throw new Error(`缺少产物: ${f}`);
}

const auth = creds();
await upload(repo, setupName, setupPath, auth);
await upload(repo, setupName + '.blockmap', blockmapPath, auth);
await upload(repo, ymlName, ymlPath, auth);
console.log('dist:push ok', `${PUBLIC_URL}${ymlName}`);

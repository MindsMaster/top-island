import { spawn } from 'child_process';
import { createServer } from 'vite';
import electronPath from 'electron';
import { buildElectron } from './build.mjs';

await buildElectron();

const server = await createServer();
await server.listen();
const url = server.resolvedUrls.local[0];
console.log(`[dev] vite dev server: ${url}`);

const child = spawn(String(electronPath), ['.'], {
  stdio: 'inherit',
  env: { ...process.env, VITE_DEV_SERVER_URL: url },
});

child.on('exit', async (code) => {
  await server.close();
  process.exit(code ?? 0);
});

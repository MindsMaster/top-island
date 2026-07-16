import { createBridge, Bridge } from './winbridge';

let bridge: Bridge | null = null;
let disposed = false;
let onChange: (() => void) | null = null;

function ensureBridge(): Bridge | null {
  if (disposed) return null;
  if (!bridge) {
    bridge = createBridge({
      mode: 'clip',
      tag: 'Clipboard helper',
      onEvent: (event) => {
        if (event === 'clipboard') onChange?.();
      },
      onSpawn: () => void bridge?.request('watch'),
    });
  }
  return bridge;
}

export function startClipboardEvents(cb: () => void) {
  onChange = cb;
  ensureBridge();
}

export function disposeClipboard() {
  disposed = true;
  bridge?.dispose();
  bridge = null;
}

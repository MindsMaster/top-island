import type { IslandApi } from '../shared/ipc';

declare global {
  interface Window {
    islandAPI: IslandApi;
  }
}

export {};

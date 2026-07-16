import { app } from 'electron';
import * as path from 'path';

export function resourcePath(name: string): string {
  return app.isPackaged
    ? path.join(process.resourcesPath, name)
    : path.join(app.getAppPath(), 'resources', name);
}

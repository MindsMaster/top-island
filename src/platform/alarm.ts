import { call } from './invoke';
import type { AlarmSound } from './types';

export const alarmApi = {
  /** %windir%\Media 下 Alarm*.wav */
  listSounds: () => call<AlarmSound[]>('alarm_sound_list'),
  soundData: (path: string) => call<ArrayBuffer>('alarm_sound_data', { path }),
  /** 取消返回 null */
  pickSound: () => call<AlarmSound | null>('alarm_sound_pick'),
};

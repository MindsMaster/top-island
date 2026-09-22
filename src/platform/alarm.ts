import { call } from './invoke';
import type { AlarmSound } from './types';

export const alarmApi = {
  /** 系统默认闹钟音（%windir%\Media\Alarm*.wav） */
  listSounds: () => call<AlarmSound[]>('alarm_sound_list'),
  /** 音频文件 → data URL（渲染层 file:// 受限，经后端转运） */
  soundData: (path: string) => call<string | null>('alarm_sound_data', { path }),
  /** 打开文件对话框选择自定义音频；取消返回 null */
  pickSound: () => call<AlarmSound | null>('alarm_sound_pick'),
};

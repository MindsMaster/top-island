export type Lang = 'zh-CN' | 'en-US';

export interface Messages {
  [key: string]: string | string[] | Record<number, string>;
  weekdays: string[];
  weatherCodes: Record<number, string>;
}

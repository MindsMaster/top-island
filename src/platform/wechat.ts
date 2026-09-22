import { call } from './invoke';

export interface AcquireKeyResult {
  ok: boolean;
  wxid?: string;
  /** 稳定错误码：no-account / recover-failed */
  error?: string;
}

export const wechatApi = {
  /** 获取并缓存解密密钥（扫内存，需 Weixin.exe 在运行） */
  acquireKey: () => call<AcquireKeyResult>('wechat_acquire_key'),
  hasKey: () => call<boolean>('wechat_has_key'),
};

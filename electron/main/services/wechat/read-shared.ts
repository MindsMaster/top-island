import * as crypto from 'crypto';
import * as zlib from 'zlib';

export interface ContactInfo {
  username: string;
  name: string;
  avatar: string; // small_head_url（http），渲染层经 notifyImage 抓取
  isGroup: boolean;
  muted: boolean;
}
export interface Contacts {
  byUsername: Map<string, ContactInfo>;
  byHash: Map<string, ContactInfo>;
}
export interface RawMessage {
  table: string;
  localId: number;
  createTime: number;
  localType: number;
  senderUsername: string;
  content: string;
  isSend: boolean;
}

export function md5Hex(s: string): string {
  return crypto.createHash('md5').update(s, 'utf8').digest('hex');
}

/** 群聊文本正文常带 "wxid_xxx:\n" 前缀，剥掉 */
export function stripSenderPrefix(content: string): string {
  const m = /^(wxid_[0-9a-zA-Z_-]+|gh_[0-9a-zA-Z_-]+|[0-9]+@chatroom):\n?/.exec(content);
  return m ? content.slice(m[0].length) : content;
}

/**
 * 消息类型占位。local_type 是微信4.x 的 64 位组合值（子类型<<32|基类型），拍一拍等在高位。
 * 返回 null 表示文本，用真实内容。
 */
export function placeholderFor(localType: number): string | null {
  switch (localType) {
    case 1:
      return null;
    case 3:
      return '[图片]';
    case 34:
      return '[语音]';
    case 43:
      return '[视频]';
    case 47:
      return '[动画表情]';
    case 42:
      return '[名片]';
    case 48:
      return '[位置]';
    case 49:
      return '[链接/文件]';
    case 50:
      return '[通话]';
    case 10000:
      return '[系统消息]';
    case 244813135921:
      return '[引用消息]';
    case 17179869233:
      return '[链接]';
    case 21474836529:
      return '[文章]';
    case 154618822705:
      return '[小程序]';
    case 12884901937:
      return '[音乐]';
    case 8594229559345:
      return '[红包]';
    case 81604378673:
      return '[聊天记录]';
    case 266287972401:
      return '[拍一拍]';
    case 8589934592049:
      return '[转账]';
    case 270582939697:
      return '[直播]';
    case 25769803825:
      return '[文件]';
    default:
      return '[消息]';
  }
}

/** message_content 可能是明文字符串或 BLOB（部分 zstd 压缩，魔数 28 B5 2F FD） */
export function decodeContent(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  if (v instanceof Uint8Array) {
    const b = Buffer.from(v.buffer, v.byteOffset, v.byteLength);
    if (b.length >= 4 && b[0] === 0x28 && b[1] === 0xb5 && b[2] === 0x2f && b[3] === 0xfd) {
      try {
        return zlib.zstdDecompressSync(b).toString('utf8');
      } catch {
        return '';
      }
    }
    return b.toString('utf8');
  }
  return String(v);
}

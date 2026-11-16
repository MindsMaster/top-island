# Top Island

[![CI](https://github.com/MindsMaster/top-island/actions/workflows/ci.yml/badge.svg)](https://github.com/MindsMaster/top-island/actions/workflows/ci.yml)
[![邮箱](https://img.shields.io/badge/%E9%82%AE%E7%AE%B1-an5w1r%40163.com-blue.svg)](mailto:an5w1r@163.com)
[![Discord](https://img.shields.io/badge/Discord-5865F2.svg?logo=discord&logoColor=white)](https://discord.invite.azuramc.cc)
[![QQ群](https://img.shields.io/badge/QQ%E7%BE%A4-1046048297-12B7F5.svg?logo=qq&logoColor=white)](https://qun.qq.com/universal-share/share?ac=1&authKey=QEsKQUh0Z2dhNgn2lQDrnRRGD0MALQvTfF3FPr5ZYOv8i/lKdNp8G6gjTgYrD/Rw&busi_data=eyJncm91cENvZGUiOiIxMDQ2MDQ4Mjk3IiwidG9rZW4iOiJRR3lINzRUTUt0U2M0bDRkeTRhTERCT0ZscFlsTUxBNm9TdjUwelYxZXN2dzdUWUlyWnY0Mi9oOHI3c2JBOEYrIiwidWluIjoiMTE3MzM0ODYxMCJ9&data=EuNu7PewjDdMH0GF2GuEOYyIXT8iT25RJajcGVD3qL5LXkiATxSyzB-gdxqgbbFkbRvkyzoiJQ_sJ8-tBfFbSuW4NYFdjzdIr5Z1E8reAiM&svctype=5&tempid=h5_group_info)

Windows 桌面顶部的灵动岛式悬浮组件，基于 Tauri 2、Vue 3 与 Rust。

## 功能

- 天气：自动定位或按选定城市显示
- 音乐：通过系统媒体控制显示播放状态，网易云音乐、酷狗音乐、QQ 音乐额外接入歌词
- 待办与日历
- 闹钟
- 剪贴板历史
- 消息：系统通知中心与微信 4.x 消息提醒

## 系统要求

- Windows 10 / 11 x64
- WebView2 运行时

## 开发

### 环境

- Node.js 24+
- pnpm 12
- Rust stable，MSVC 工具链（由 `rust-toolchain.toml` 指定）
- Visual Studio Build Tools，含「使用 C++ 的桌面开发」工作负载

### 启动

```powershell
pnpm install
pnpm dev
```

`pnpm dev` 实际执行 `cargo xtask dev`：先以 release 编译播放器 bridge DLL，再自 1420 起寻找空闲端口启动 `tauri dev`。

### 检查

```powershell
pnpm typecheck
pnpm lint
pnpm test
pnpm build
pnpm format:check
cargo check --workspace
cargo test --workspace
```

格式化使用 `pnpm format`，同时覆盖前端与 Rust 代码。

## 参与贡献

欢迎提交 Issue 与 Pull Request。

- 缺陷报告请附系统版本、复现步骤与截图
- 较大的功能改动建议先开 Issue 讨论方案
- 从 `master` 拉分支开发，提交前在本地跑通上文「检查」中的命令
- 编码、注释与提交信息规范见 [AGENTS.md](AGENTS.md)，提交信息格式为 `type(scope): 一句话`
- PR 会触发 CI，全部通过后合入

## 交流

- QQ 群：[1046048297](https://qun.qq.com/universal-share/share?ac=1&authKey=QEsKQUh0Z2dhNgn2lQDrnRRGD0MALQvTfF3FPr5ZYOv8i/lKdNp8G6gjTgYrD/Rw&busi_data=eyJncm91cENvZGUiOiIxMDQ2MDQ4Mjk3IiwidG9rZW4iOiJRR3lINzRUTUt0U2M0bDRkeTRhTERCT0ZscFlsTUxBNm9TdjUwelYxZXN2dzdUWUlyWnY0Mi9oOHI3c2JBOEYrIiwidWluIjoiMTE3MzM0ODYxMCJ9&data=EuNu7PewjDdMH0GF2GuEOYyIXT8iT25RJajcGVD3qL5LXkiATxSyzB-gdxqgbbFkbRvkyzoiJQ_sJ8-tBfFbSuW4NYFdjzdIr5Z1E8reAiM&svctype=5&tempid=h5_group_info)
- Discord：[discord.invite.azuramc.cc](https://discord.invite.azuramc.cc)
- 邮箱：[an5w1r@163.com](mailto:an5w1r@163.com)

## 项目结构

```
src/              前端：岛壳、功能模块、设置窗口
src-tauri/        Tauri 应用：IPC、后台服务、托盘与窗口
crates/
  island-core/              共享领域类型
  island-windows/           Windows 系统集成：SMTC、通知中心、剪贴板、输入钩子
  island-wechat/            微信本地消息读取
  island-cloudmusic-bridge/ 网易云音乐代理 DLL
  island-kugou-bridge/      酷狗音乐代理 DLL
xtask/            开发、构建、发布脚本
build/            NSIS 安装器钩子
```

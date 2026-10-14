# Framecraft · 对标剪辑助手

基于 Tauri、React 和 Rust 的桌面视频创作工具，面向 macOS 和 Windows，已在 M1 Mac 上测试。支持中文和英文界面。

- 分析对标视频的画面、文案与叙事结构，按新主题生成脚本。
- 按文件夹管理工作空间，从原始素材生成可审阅的剪辑方案，导出 MP4 和 SRT。
- 使用 Claude Code 本地登录或兼容 Anthropic 的 API，可验证 CLI 登录状态。
- 支持字节跳动配音、Seedream 图像生成，以及 Google Vertex AI 密钥或本地 ADC 登录。
- Mac 使用 VideoToolbox 硬件编码，支持时启用硬件缩放；密钥保存在系统凭据库。

## 启动

需要 Node.js 22+、Rust、[Tauri 平台依赖](https://v2.tauri.app/start/prerequisites/)、Claude Code 和 FFmpeg/FFprobe。Whisper 可选。

```sh
npm ci
npm run tauri -- dev
```

Mac 推荐使用 `./script/build_and_run.sh` 构建并运行。脚本会设置稳定的本地签名；首次 Keychain 提示选择「始终允许」，后续重建可保留授权。此签名仅用于本地开发，不包含公证。

在「连接设置」配置服务和自己的音色 ID，然后通过「添加工作空间」选择文件夹开始创作。支持导入 AIGC Video Desktop 已保存的 API 密钥。

## 构建与测试

```sh
npm run tauri -- build
npm test
npm run test:media  # FFmpeg 实际渲染测试
```

GitHub Actions 提供 macOS / Windows 构建。

当前导出支持直切、画幅适配、原音与独立字幕；生成的配音和图片暂不自动加入时间线。Mac 不回退到 CPU 视频编码，音频处理和画面填充仍使用 CPU；Windows 暂用软件编码。

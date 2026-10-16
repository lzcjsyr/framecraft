# Framecraft

A small desktop video studio built with Tauri, React, and Rust.

**Guidance + an assets folder + a conversation with Claude Code → video.**

Describe the video, optionally add a reference video and Markdown skills, and choose **one assets folder** containing all related content. Click **Open Claude Code**, then ask the CLI to make or edit the video. The [terminal agent instructions](templates/terminal-agent.md) connect the live goal and folder to the agent's tools. A reference video is optional.

The interface has three areas: guidance and the assets folder on the left, a video preview in the middle, and the **official interactive Claude Code CLI** on the right. This is an xterm.js terminal backed by a native PTY, with the CLI's own input, slash commands, permissions, tool output, and available thinking. `Ctrl+O` opens its transcript viewer; `Esc` interrupts. Ask for edits such as “shorten the ending” or “make it portrait.” Questions do not need a render. Each render creates a new version; select an earlier preview to edit from it.

If Claude Code is missing, the right panel shows **Install Claude Code** and **Already installed? Check again**. Installation uses the [official native installer](https://code.claude.com/docs/en/setup#install-claude-code), shows progress and retryable errors, verifies the installed CLI, and saves its executable path. Existing installations are reused. No Node.js or npm setup is required for a new native installation.

The terminal panel selects local Claude login or a third-party API, including MiMo. Media providers and executable paths remain in Settings. The CLI starts in the selected assets folder and resumes its saved native session when available. Changing connection or folder stops the terminal; the next start uses the new settings. Conversations remain in Claude Code's history. The app's local MCP server supplies `project_context`, `generate_asset`, `transcribe_asset`, `render_video`, and `publish_video`, sharing the app's credentials, task cancellation, and preview updates. Guidance saves while the terminal is open; tools read the latest selected preview version.

Finished videos and SRT files are saved **directly in the chosen assets folder**, as `framecraft-<UUID>.mp4` / `.srt`. Earlier versions and source files are not overwritten by the renderer. These filenames and partial renders are excluded from input scanning across workspaces. Working files and version metadata remain in the app workspace. **Update** updates the actual selected CLI installation, including its npm prefix or Homebrew cask; stop the CLI and media jobs first. This button updates Claude Code, not Framecraft itself.

## Run

Requires Node.js 22+, Rust, [Tauri platform dependencies](https://v2.tauri.app/start/prerequisites/), Claude Code, and FFmpeg/FFprobe.

```sh
npm ci
npm run tauri -- dev
```

On macOS, `./script/build_and_run.sh` builds and launches an ad hoc signed development app. Default signing does not access Keychain or create a certificate. Set `FRAMECRAFT_SIGNING_IDENTITY` only when you explicitly want a particular signing identity. This is not notarized distribution signing. Development API credentials use the local `.env` file.

## Providers

Development builds read and save API keys in the repository's Git-ignored `.env` file, including when the built Mac app is launched from Finder. Saving a key in Settings or the terminal connection controls updates this file with owner-only permissions (`0600` on macOS/Linux). Blank fields keep existing keys. There is no Keychain fallback for development provider keys, so missing keys produce a setup message instead of an unlock prompt. Local Claude login uses the installed CLI's existing account credentials.

Copy `.env.example` to `.env` when setting up a new checkout, or enter keys in Settings. Process environment variables take precedence. `FRAMECRAFT_ENV_FILE` can select another local credential file. Release builds retain the system vault unless that override is set. The UI receives only presence flags; keys never enter the agent prompt or command arguments.

- **Agent:** Claude Code with local account login, MiMo at `https://api.xiaomimimo.com/anthropic` (default `mimo-v2.5`), or a custom Anthropic-compatible endpoint using `AGENT_API_KEY`. API credentials and model aliases are set for that child process only. Global Claude configuration is unchanged.
- **TTS:** `mimo-v2.5-tts`, with built-in Chinese and English voices. The agent can generate narration through its media tools and reuse it in later edits.
- **ASR:** `mimo-v2.5-asr`. Audio and video without transcripts are transcribed in 60-second segments. Segment timing is approximate; word-level timestamps are unavailable. If transcription fails, the agent receives an explicit warning.
- **Images:** Seedream through Volcengine, or Google Vertex AI through a local Google ADC login. Google uses `gcloud auth application-default login` and the configured project or ADC quota project. Default image models are editable in Settings.

Official references: [MiMo with Claude Code](https://mimo.mi.com/docs/integration/claudecode), [MiMo TTS](https://mimo.mi.com/docs/en-US/quick-start/usage-guide/audio/speech-synthesis-v2.5), [MiMo ASR](https://mimo.mi.com/docs/en-US/api/audio/Speech-Recognition), [Seedream API](https://docs.volcengine.com/docs/ark/image-generation-api?lang=en), and [Google image generation](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/capabilities/image-generation).

Terminal integration follows the official [interactive CLI](https://code.claude.com/docs/en/interactive-mode), [CLI reference](https://code.claude.com/docs/en/cli-reference), and [MCP integration](https://code.claude.com/docs/en/mcp). No VS Code extension is copied or hosted. The older structured pipeline remains available to integration tests.

## Supported output

The built-in renderer supports straight cuts between clips and held images, source audio or a supplied soundtrack/narration, aspect fitting, and separate SRT captions. Captions are not burned into the picture. For other editing work the CLI may use normal file and command tools, then call `publish_video` to register a finished MP4 in preview/history.

Supports 16:9, 9:16, and square output. macOS uses VideoToolbox hardware video encoding; Windows uses FFmpeg software encoding. The folder index supports video, images, audio, TXT, Markdown, SRT, VTT, JSON, and CSV. Claude Code can inspect other related documents with its Read tool; unsupported or irrelevant files should be explained in the result warnings. Hidden files, dependency directories, and app output directories are excluded from the media index. If the selected folder moves, choose its new location.

## Tests

```sh
npm run build
npm test
npm run test:media
npm run test:mixed
npm run test:speech   # real MiMo TTS → ASR; needs MIMO_API_KEY
npm run test:live     # real Claude Code + MiMo + Google ADC → MP4; billable
npm run test:folder   # real Claude Code inspects image + audio + text folder → MP4
npm run test:conversation # real create → chat edit → question → earlier-version portrait edit
npm run test:local-agent  # small real request through the existing Claude account login
npm run test:seedream # real Seedream request; needs a saved key or VOLCENGINE_API_KEY
```

`FRAMECRAFT_TEST_OUTPUT` optionally chooses a new folder for a live video test. The folder and conversation tests build their image/audio fixtures with FFmpeg and MiMo TTS. To reuse supplied fixtures, set `FRAMECRAFT_TEST_ASSETS` to a folder containing `images/sunrise.jpg` and `narration.wav`. Provider tests make small real requests. Regular tests do not call providers. See [verification notes](docs/verification.md) and the [conversation design concept](docs/conversation-concept.png).

For headless MCP checks, the built executable also accepts `--mcp-stdio <storage-directory> <registered-workspace-id>`. Use isolated test storage, separate from a running GUI. Production terminals use a stdio proxy to the running app, so workspace writes and preview events stay in one process.

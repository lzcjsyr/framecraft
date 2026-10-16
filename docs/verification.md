# Framecraft redesign and verification

## First-time CLI setup — 2026-10-16

The right terminal panel detects CLI availability on startup, executable-path changes, and window focus. A missing executable shows Install Claude Code and Already installed? Check again before authentication controls. Installation uses the official native installer, streams progress, reports retryable errors, verifies `--version`, and saves the resolved executable path. An already installed CLI is reused, including recovery from a stale configured path. Start and updates share the installation guard.

Native macOS QA used a temporary nonexistent executable path to exercise the missing state without removing installed tools. The installation card and recheck control rendered correctly. Clicking Install found the existing CLI, verified version 2.1.301, saved its path, restored authentication and Start controls, and successfully launched the real CLI. The original automatic path-detection preference was restored. Evidence: `/tmp/framecraft-cli-install-missing.png` and `/tmp/framecraft-cli-install-ready.png`. Visual QA caught and fixed low contrast on the install button.

The official install script was fetched successfully and inspected, including its platform selection and SHA-256 verification. This host already has Claude Code, so a completely fresh download-and-install was not executed, nor were Windows/Linux installers. Those paths remain unverified on clean machines. No existing CLI was removed or reinstalled to simulate absence.

TypeScript/Vite production build, native bundle, signature verification, and all 20 regular Rust tests passed (nine optional media/provider tests excluded). The existing localhost preview remained nonblank with the correct identity, no framework overlay, and no console warnings/errors. Native installation interactions were verified through the actual app window, rather than a browser mock.


## Official interactive terminal — 2026-10-16

This implementation supersedes the custom chat described below. The right panel now hosts the unmodified official Claude Code CLI through xterm.js and a native PTY. The CLI handles input, slash commands, approvals, tool output, available model thinking, and its own conversation history. Ctrl+O opens its detailed transcript. Start/Stop resumes a saved native session when present; changing the provider or assets folder starts the appropriate session. Guidance and the currently selected preview are read live through the app's MCP tools.

The desktop layout keeps Guidance/assets on the left, Preview in the middle, and Terminal on the right. Native minimum width is 980 px; narrow browser previews scroll horizontally instead of moving Terminal below Preview. Browser geometry checks passed at 1440 × 940 and 980 × 640. The final native window was also inspected. Evidence: `output/terminal-right-layout.png` and `output/terminal-native-verified.png`.

Twenty regular Rust tests passed, with nine optional provider/media tests excluded from that run. The new real PTY test verifies ANSI output, Chinese input, terminal resize, output replay, and child-process termination. TypeScript, Vite production build, the native macOS bundle, and strict ad hoc signature verification passed. The real mixed FFmpeg test separately verified output directly in assets, SRT timing, source rescanning, and preserving a second output version.

A genuine interactive Claude Code 2.1.301 session using MiMo passed creation (four seconds), a conversational edit (two seconds and changed subtitle), a duration question without another export, and Ctrl+O detailed transcript. The CLI retained its full built-in tools. Its MCP tools used the same renderer and workspace code as the app. Both MP4/SRT versions are directly in `output/terminal-verified/related-content/`; the original remains intact. Evidence: that workspace manifest, native Claude session history, and the sanitized `interactive-transcript.txt`.

This test found that the interactive CLI could omit MCP definitions on a compatible third-party endpoint despite the server connecting. Setting the [documented](https://code.claude.com/docs/en/env-vars) `ENABLE_TOOL_SEARCH=false` for third-party connections made the actual request contain all five Framecraft schemas, while preserving the CLI's built-in tools. Framecraft's server is also marked `alwaysLoad`. Diagnostics recorded only tool names; credentials were never logged. The production native check below connected directly to MiMo.

Native end-to-end verification passed: starting the right-side CLI, accepting its normal test-folder trust prompt and tool approvals, asking about the selected four-second preview without rendering, then asking in the same conversation for a three-second version with subtitle `右侧终端连续剪辑`. The app automatically selected Version 5 and played its three-second H.264/AAC 1920 × 1080 video. Its MP4 and SRT are directly in the selected `output/conversation-verified/related-content/` folder, and all earlier versions remain. The CLI's actual progress, thinking output, tool inputs/results, and reply appeared in the terminal.

The native **Update** button completed successfully and reported Claude Code **2.1.301**. Updates target the selected installation, including an explicit npm prefix, so another Node installation on PATH cannot redirect the update. Stop → Start preserved native session `570c289f-b745-4b54-9331-72b7576f722b` and restored its conversation. The native Details button switched to the CLI's “Showing detailed transcript” view and back to normal input. The app is left running with the tested three-second preview.

Development credentials remain in Git-ignored `.env` with 0600 permissions. MiMo authentication uses its documented token environment variable; local-account mode removes provider overrides. No repeated unlock or Keychain request was needed during native terminal verification. Seedream remains unverified without a usable saved development key; earlier Google ADC and MiMo speech checks are recorded below.


## Conversational editor — 2026-10-16

The primary screen now has Guidance and one assets folder on the left, Preview in the middle, and an interactive Claude Code conversation on the right. Messages and version links persist in `workspace.json`. Each editing request receives the selected version's resolved edit plan and current source IDs. New renders create new exports; questions return a reply without an export. Existing generated images and voice tracks remain available for later edits.

The agent resumes an explicit Claude Code session. Current goal, folder path, selected version, and recent conversation are supplied directly on stdin as well as saved in the run context. Provider changes use a fresh native session with the saved conversation context. Native sessions and credentials stay on the user's machine. Local Claude account login and third-party API modes are selectable in the right panel, with model/endpoint/key controls and a local-login status check. API keys stay in the ignored development `.env`; all provider environment changes are scoped to the CLI child process.

Nineteen regular checks passed, including Unicode context delivery to a child process, version preservation for reply-only turns, source-cache reuse, generated-asset reuse, and legacy manifest migration. The TypeScript/production frontend build and native macOS bundle pass. A real request through the existing Claude account login (`sonnet`) answered successfully without rendering.

The real MiMo/Claude Code conversation test passed all four turns in one native session, with eight persisted messages and three separate exports:

| Turn | Verified result |
| --- | --- |
| Create from folder | 6 seconds, 1920 × 1080; supplied photo, audio, and Markdown instructions. |
| Chat edit | 4 seconds, same image/audio, changed SRT caption; original MP4 preserved. |
| Question | Answered that the selected version is 4 seconds; export count stayed at two. |
| Edit selected original | Kept the original six-second duration and sources, changed caption, and rendered 1080 × 1920 portrait; export count became three. |

Evidence: `output/conversation-verified/{result,revision,branch}.json`, its saved workspace, and review copies `output/conversation-original.mp4`, `conversation-edited.mp4`, and `conversation-portrait.mp4`. The source IDs and native session ID were asserted across turns, and the workspace was reloaded between creation and the first edit.

Live testing caught MiMo returning the string `"null"` for a nullable structured-output field. The response contract now uses an explicit `reply` or `render` action with a consistently shaped storyboard object. It also limits selectable source IDs to the actual indexed IDs. Invalid source references are rejected before rendering. CLI result diagnostics are saved with each run.

The native app detected the `.env` MiMo key, opened the tested workspace through its folder picker, restored all eight messages and three versions, selected Version 2, and played its four-second video in the middle preview. A question sent through the actual native chat answered about Version 2 without adding an export. This check exposed a preview-selection reset on send; the UI now keeps the selected version while a request runs and selects a new version only when a new export arrives.

Final native checks passed after that fix: selecting the assets folder through Change folder saved its path and preserved all source IDs; saving Local Claude login and checking login showed the existing signed-in account; saving Third-party API restored MiMo with the existing `.env` key. Sending a Chinese question with Enter kept Version 2 selected throughout the request and produced a four-second answer without a new export. A subsequent native chat request changed only its caption, rendered a fourth version, and automatically selected Version 4. The resulting file remains four seconds at 1920 × 1080, with the same source IDs and the caption `原生对话剪辑测试。` in its SRT. The saved workspace has 14 messages, four video exports, three successful native chat runs, and one Claude Code session across the test and native turns. Review copy: `output/native-conversation-edited.mp4` and `.srt`. No unlock request or key re-entry was needed.

When the agent changes the output aspect ratio, the workspace input revision now advances so an older local draft cannot override that change after a reload. A fresh render also leaves the previously selected preview visible until the new export is ready.

The development signing helper now defaults to ad hoc signing and verifies the bundle without opening Keychain, creating a certificate, or changing certificate trust. An explicitly supplied `FRAMECRAFT_SIGNING_IDENTITY` still selects certificate signing. The existing local certificate, if any, is left in place.

The final helper was run with the identity override unset; strict `codesign` verification passed and inspection reported `Signature=adhoc`. The signed final bundle launched, restored 14 messages and Version 4, detected the local MiMo key, and played the four-second result. Native screenshot: `/tmp/framecraft-conversation-native.jpg`. The native screenshot and concept were inspected in the final visual comparison pass.

The new concept is `conversation-concept.png`, generated through the requested Google ADC image provider. Built-in-browser QA used `http://127.0.0.1:1427/`, including 1376 × 768 concept dimensions and 390 × 844 narrow layout. At 1080 × 640 with expanded connection settings, the chat composer ended at y=615, within the viewport, and document width remained 1080. Page identity, nonblank rendering, absence of a framework overlay, and console health passed. Switching local/API modes and custom provider fields updated real UI state; unapplied connection changes disabled Send, and failed browser-preview sends preserved the draft. No Playwright browser fallback was used.

Both the concept and final rendered screenshot were opened with `view_image` in one comparison pass. The implemented surface was faithfully verified against the design with the following intentional functional adjustments:

| Comparison | Rendered evidence / intentional adjustment |
| --- | --- |
| Layout | Left input cards, dominant middle preview, full-height right chat rail, and bottom composer retained. Narrow screens stack these work areas. |
| Palette and containers | White app surfaces, light gray preview, indigo actions, fine borders, restrained corners. Existing true-white window background retained. |
| Typography | System sans-serif with explicit 20px headings and compact 11–13px controls/chat text; desktop density is deliberately smaller than the illustrative concept. |
| Copy and states | Guidance, Goal, Assets, Choose folder, Preview, Claude Code, Connection settings, Agent activity, and Send retained. Actual empty state replaces illustrative example messages, path, and version number. Defaults remain the user's saved values. |
| Icons | Existing Lucide outline family retained for folders, settings, disclosure, playback, and sending; the existing wordmark is preserved. |
| Interaction and spacing | Composer remains accessible independently of transcript scroll; connection settings scroll within the rail. Version actions update the editing target. Duration text clipping found in QA was corrected. |

Above-the-fold copy changes are limited to these recorded state/functional differences. There are no invented metrics or simulated output states. Browser screenshots are kept outside the repository; the native check uses real exported media and saved messages.

## Folder-based assets update

The user's clarified workflow supersedes the earlier individual-file import and manual generation controls. Assets now has one Choose folder / Change folder action and displays the absolute selected path. Choosing it from a new session automatically creates the app's workspace outside the source folder. Create video requires a goal and folder.

`templates/video-agent.md` receives the selected folder path and goal. Claude Code has Read and Glob tools, receives the folder through `--add-dir`, lists subfolders, evaluates related content, and creates the storyboard. The app rescans supported files on each run; additions, modifications, and removals are reflected without importing files again. Existing `rawFolder` manifests migrate to `assetsFolder`. Source files stay in place.

Seventeen regular Rust tests and the TypeScript/production frontend build pass. The folder tests cover recursive discovery (including a subfolder named `assets`), hidden-file exclusion, additions/changes/removals, source ID reuse, folder validation, explicit prompt inputs, and legacy manifest migration.

A real folder run used a photo, supplied six-second narration, and Markdown instructions. Logs show MiMo ASR, Claude Code's actual Glob invocation, Read calls for the text and image, storyboard creation, and VideoToolbox rendering. The result is `output/folder-test/result.json` and its linked six-second 1920 × 1080 MP4 with supplied audio and an SRT sidecar. No replacement image or narration was generated during that run.

The reproducible `npm run test:folder` also passed with no pre-existing fixture files: FFmpeg prepared an image, MiMo TTS prepared a six-second audio fixture, and the actual agent pipeline rescanned and inspected that folder, transcribed its audio with MiMo ASR, and rendered the video. Result: `output/folder-self-contained/result.json`. A review copy from the supplied-photo test is available at `output/folder-video.mp4` and `output/folder-video.srt`; FFprobe confirmed H.264, 1920 × 1080, AAC, and exactly six seconds.

Rendered browser checks at `http://127.0.0.1:1427/` confirmed the folder control, goal editing, disabled Create video without a folder, and the explicit desktop-only picker message in browser preview. The page had no framework overlay or console warnings/errors. Desktop (1440 × 900) and narrow (390 × 844) layouts passed; narrow document width equals viewport width. The old file import and manual image/voice controls are absent. The new Assets section intentionally changes that part of the initial design concept. At this stage the native folder picker could not be checked while the Mac was locked; subsequent native checks are recorded in the conversational-editor section above.

## Product and design

The primary screen combines Guidance (prompt, optional reference video, selected skills), Assets (one local folder), and Your video. Claude Code remains the agent runtime; MiMo provides the model, speech, and recognition. The old six-screen workflow and mandatory reference-analysis stages have been removed.

The complete primary-screen concept was generated with the requested Google ADC provider, using `gemini-3.1-flash-image`. Its brief specified a white two-pane desktop studio, concise labels, indigo primary action, no sidebar, and native HTML controls. Source: `design-concept.png`, 2752 × 1536 raster pixels. Tokens: white background, #18181b text, #73737d secondary text, #dedee4 borders, #3c3486 accent, system sans-serif/Inter, 7–9px control radii, 28px pane gap. Controls and labels are code-native.

`app-desktop.png` is a screenshot from the Codex built-in browser at 1440 × 900 CSS pixels. Both concept and rendered screenshot were opened with `view_image` in the same comparison pass. Browser interaction checks used the built-in browser, without a Playwright-browser fallback.

| Comparison | Evidence and resolution |
| --- | --- |
| Information hierarchy | Guidance above Assets in the left pane, Your video in the right pane, one Create video action. The six-item sidebar is gone. |
| Palette | True white app surfaces, muted gray video canvas, indigo prompt border and primary action. The native window background was changed from cream to white. |
| Containers and spacing | Two balanced outer panels, 28px gap, thin borders, restrained corners. No decorative card grid or extra navigation. |
| Typography | Deliberately defined heading, label, button, caption, and modal scales. Compact desktop control typography is an intentional adjustment from the raster concept. |
| Icons | Consistent Lucide outline icons for media, speech, playback, disclosure, and settings. Settings uses a sliders icon as a compact native-style control. |
| Copy | Primary headings, prompt label, placeholder, reference/skill actions, import actions, result placeholder, and Create video match the concept. Intentional additions: narration checkbox, workspace dropdown, format descriptions, concrete output language, and the browser-only desktop-preview note. |
| Responsive behavior | Checked 1440 × 900 desktop and 390 × 844 narrow layout. The narrow layout stacks the panes and has no horizontal overflow. Settings remains reachable by its accessible name when its visible label is hidden. |

The implementation was visually verified against the generated design. Intentional product additions are recorded above; no unresolved clipping or horizontal-overflow defects were found. The browser's mobile screenshot compositor scaled the captured image unexpectedly, so the responsive check also inspected DOM layout and viewport/scroll widths directly. Concept native raster dimensions differ from the CSS viewport; comparison used the desktop layout at normal readable control sizes.

## Automated and real tests

- Frontend TypeScript and production Vite build passed.
- Regular Rust checks passed, including source validation, storyboard validation, credential isolation/redaction, cancellation, subprocess pipe draining, ADC error handling, and workspace interruption recovery.
- Real FFmpeg test passed for sequential video clips with original/silent audio and SRT timing.
- Real mixed-asset test passed for a held image, separate soundtrack with a source offset, imported Markdown, frame-rounded output duration, and invalid soundtrack/text-as-visual rejection.
- Real MiMo model request returned the expected response.
- Real MiMo TTS → MP3 conversion → MiMo ASR test recognized the original Chinese sentence.
- Real default-skills Claude Code → MiMo storyboard → Google ADC image generation → MiMo narration → VideoToolbox render passed. The resulting 6-second MP4 was probed as H.264, 1920 × 1080, with AAC audio and an SRT sidecar. The rendered image was visually inspected.
- Built-in-browser checks passed for prompt editing, aspect selection, draft recovery after reload, generation dialog, provider switching, and settings modal dismissal.

The demo is saved in `output/quiet-morning.mp4`, with `quiet-morning.srt` and a preview JPEG. Local renders are ignored by Git. Source scripts never contain the provided API key.

The MiMo key was saved and read back successfully in a dedicated system Keychain item. App settings now select Claude Code + MiMo and the verified Google ADC image provider; previous settings were backed up. First access from the rebuilt app can require normal macOS Keychain authorization.

Native app-window inspection and a live Seedream call could not be completed while the Mac was locked and the legacy credential vault was inaccessible. An unlock request was sent while independent work continued. The signed macOS app bundle builds successfully; native-window playback and Seedream service access are explicitly unverified. No API-key strings were found in tracked source, and the temporary plaintext test credential was removed before delivery.

## Development credential update

At the user's request, development builds now use the repository `.env` file with owner-only permissions. Environment variables take precedence; missing entries do not fall back to Keychain. Settings saves to the same file, and external `.env` edits are read on the next provider operation without restarting. `.env.example` contains variable names only. The production vault remains available for release builds.

After the Mac was unlocked, the native app's connection screen confirmed MiMo credentials, Claude Code, FFmpeg/FFprobe, and Google ADC. Further native checks use the rebuilt `.env` version.

The local `.env` path passed a real MiMo TTS → ASR test with `MIMO_API_KEY` removed from the test process environment. Fifteen regular tests passed, including local-file parsing, preservation of other configuration, blank-key handling, secret-safe errors, and `0600` permissions. The rebuilt native app uses the same credential file. Seedream migration was attempted only with system interaction disabled; unavailable access was skipped without prompting.

Native verification after rebuilding: connection checks detected the `.env` MiMo key, Claude Code, FFmpeg/FFprobe, and Google ADC without Keychain access. An isolated `native-check` workspace was created, the generated MP4 was imported through the native file picker, and its player showed Pause and a six-second duration after playback started. Screenshot: `app-native-playback.png`. Seedream remains unverified because its legacy key could not be read noninteractively; no authorization prompt was requested.

# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend    # cargo fmt only
```

**Model Setup (Required for Development):**

```bash
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

Handy is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
  - `mobile.rs` - Handy Mobile: the server paired phones and computers dictate through (see [Handy Mobile](#handy-mobile))
  - `remote.rs` - Transcribing with another computer: this app as a client of another Handy's server (see [Handy Mobile](#handy-mobile))
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Recording sink (VAD, levels, live streaming feed), output devices, WAV I/O
  - `vad/` - Voice Activity Detection (Silero VAD)
- `commands/` - Tauri command handlers for frontend communication
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut.rs` - Global keyboard shortcut handling
- `settings.rs` - Application settings management
- `notices.rs` - Reports background errors to the user (see [Error notices](#error-notices))
- `overlay.rs` - Recording overlay window (platform-specific)
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `utils.rs` - Platform detection helpers

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
  - `model-selector/` - Model management interface
  - `onboarding/` - First-run experience
  - `overlay/` - Recording overlay UI
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta)
- `overlay/` - Recording overlay window entry point
- `lib/notices.ts` - Maps each notice code to its text
- `stores/noticeStore.ts` - The one place notices are handled: what's active and what's on screen
- `components/notices/NoticePanel.tsx` - The one place notices are drawn, as a toast or the full list
- `lib/types.ts` - Shared TypeScript type definitions

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → VAD → Whisper/Parakeet → Text output → Clipboard/Paste

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `transcribe-cpp` - Local speech recognition (Whisper, Parakeet, Moonshine, SenseVoice, etc.; GGML/GGUF) with GPU acceleration
- `handy-recorder` - Microphone capture: device selection, resampling to 16 kHz, failure detection
- `transcribe-cpp` (Silero VAD v6, CPU-only, in-process) / `earshot` - Voice Activity Detection
- `rdev` - Global keyboard shortcuts
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads preferred Whisper model (Small/Medium/Turbo/Large)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options

### Single Instance Architecture

The app enforces single instance behavior — launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Handy Mobile

A phone is a remote microphone: it streams a recording to the desktop, which transcribes it and pastes the text or sends it back. Another computer can pair the same way and use the desktop as its engine.

- `crates/handy-link/` - Shared by both apps. `protocol.rs` (messages), `pairing.rs` (the QR payload and secrets), `tls.rs` (self-signed identity, fingerprint pinning), `discovery.rs` (finding the desktop after its address changed), `server.rs` (feature `server`, the desktop) and `client.rs` (feature `client`, the phone). `tests/roundtrip.rs` runs the whole exchange over a real socket; `examples/link-cli.rs` is a phone in a terminal for testing a desktop.
- `src-tauri/src/managers/mobile.rs` - Implements `handy_link::server::Host`: owns the server, pairing codes and the paired phones, and runs a phone's recording through `transcribe_in_chunks` like a file job. Phones and the certificate are stored in `mobile_link.json` in the app data directory, not in settings.
- `src-tauri/src/commands/mobile.rs` and `src/components/settings/mobile/MobileSettings.tsx` - The Mobile tab.
- `mobile/` - The phone app, a separate Tauri 2 project (see `mobile/README.md`). Its Rust core holds the link; the WebView only captures audio and draws.
- `mobile/src-tauri/gen/android/app/src/main/java/com/pais/handy/mobile/HandyKeyboardService.kt` - The Android keyboard: native, no WebView. It records and types; its link is `mobile/src-tauri/src/keyboard.rs`, called through `PhoneLink.kt`, on its own connection (channel `keyboard`). `mobile/src/screens/KeyboardSetup.tsx` walks the user through the permissions, with `SetupBridge` in `MainActivity.kt` reporting where they stand.
- `mobile/src-tauri/gen/android/app/src/main/java/com/pais/handy/mobile/HandyBubbleService.kt` - The Android floating bubble: an accessibility service that shows a microphone beside any focused text field, drawn by `Blob.kt`. Same link code as the keyboard, on its own connection (channel `bubble`).

Another computer as the engine:

- `src-tauri/src/managers/remote.rs` - The client side, on the computer without the model. While `transcription_engine` is `remote` it keeps a link to the selected paired computer (`remote_server_id`; the secrets are in `remote_servers.json`, not in settings). `actions.rs` sends a dictation's recording there when it ends, file jobs go chunk by chunk, and this computer's model stands in when `remote_fallback` allows (`RemoteFellBack` says so).
- It asks for a raw transcription (`StartOptions { raw: true }`) with its text settings (`TextOptions`: language, custom words, filler words, Chinese script). The server transcribes the whole recording as a dictation here would (`transcribe_for_device`), applies those settings, and keeps nothing: no WAV, history, post-processing, hook or paste. Post-processing, history and the paste stay on the client.
- What a device may do is decided by the pairing code the server made (`DeviceKind`), not by what the device says: a computer's code pairs it without `can_type`, so it can't paste on the server. The user can change that per device.
- `src-tauri/src/commands/remote.rs` and `src/components/settings/remote/` - The engine switch and paired computers on the Models tab; `src/stores/remoteStore.ts` holds the status.

Rules:

- A change either side could misread bumps `PROTOCOL_VERSION`. Optional fields and new messages don't; what a peer can do beyond the first protocol 2 is listed in its handshake (`protocol::feature`), and a client checks for a feature before relying on it. Unknown JSON messages are ignored by both sides.
- Dictation at the computer goes first; a phone's transcription waits for it, as file jobs do. One phone dictates at a time.
- The server is off unless `mobile_enabled` is set, and nothing before authentication is trusted: keep it that way.
- A server always transcribes with its own model, whatever it itself dictates with: no request is passed on to a third computer.

## Error notices

`notices::raise` is the one way to report a failure that happens while the user isn't looking at the settings window. Don't add a new event, toast or overlay state for one. A new error should take a few lines and need no new UI.

**Adding a new error:**

1. Add a `NoticeCode` in `src-tauri/src/notices.rs`, with its level in `NoticeCode::level` and its title in `NoticeCode::tray_title`. **Error** means nothing came out, or Handy can't do its job. **Warning** means the user got a result, but not the one they set up.
2. Add the title as `errors.<name>Title` in `src/i18n/locales/en/translation.json`, in Title Case and using the app's terms, then add the key to the other locales (`bun run check:translations` fails otherwise). The `Title` suffix is what makes `build.rs` give the tray `strings.<name>_title`. A dictation title has to fit the overlay pill, at most 215px of 12px text.
3. Map the code to its title and optional explanation in `NOTICE_DISPLAY` in `src/lib/notices.ts`. TypeScript won't compile until you do.
4. Call `notices::raise` where the failure happens, and regenerate `bindings.ts` by running the app in debug:

```rust
notices::raise(
    app,
    Notice::new(NoticeCode::PasteFailed, NoticeSource::Dictation),
);
```

**Rules:**

- Rust sends codes, never display strings. Use `with_detail` for what only Rust knows, such as a model name or a raw error.
- Handy keeps one notice per `NoticeSource`, and a newer one replaces it. A notice ends when its source next succeeds or when the user dismisses it in the footer. When a source next succeeds, call `notices::clear` before raising anything about that session, or the clear wipes the session's own notice.
- A message from the frontend goes through `notify` in `stores/noticeStore.ts`, the app's only toast. Without a `key` it's a click's result and ends with its pop-up, such as a failed copy. With one it stays in the footer until dismissed or cleared, such as a file Handy can't read. It's text the frontend already translated, so it needs no `NoticeCode`, and it doesn't show in the tray.
- A dictation notice replaces the overlay, so call `raise` instead of hiding the overlay. If a newer session owns the overlay by then, call `raise_without_overlay`.
- A raised notice pops up in the window as a toast. Its X dismisses the notice; otherwise the toast times out and the notice stays in the tray and footer, where the user can bring it back up. During onboarding it pops up below the top bar, and the footer lists it once onboarding ends. Only dictation notices show on the overlay.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX).

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Use in component: `const { t } = useTranslation(); t('key.path')`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── de/, es/, fr/, ja/, ru/, zh/, ...
    └── ...
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`

## CLI Parameters

Handy supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on/off on a running instance              |
| `--toggle-post-process`  | Toggle recording with post-processing on/off               |
| `--cancel`               | Cancel the current operation on a running instance         |
| `--start-hidden`         | Launch without showing the main window (tray icon visible) |
| `--no-tray`              | Launch without system tray (closing window quits the app)  |
| `--debug`                | Enable debug mode with verbose (Trace) logging             |

**Key design decisions:**

- CLI flags are runtime-only overrides — they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts
- **Windows**: Vulkan acceleration, code signing. Implicit Vulkan layers (overlays, capture hooks) are disabled for the Handy process via `VK_LOADER_LAYERS_DISABLE=~implicit~` set in `main.rs`; opt out with `HANDY_KEEP_VULKAN_IMPLICIT_LAYERS=1` or by setting `VK_LOADER_LAYERS_DISABLE` yourself
- **Linux**: OpenBLAS + Vulkan, limited Wayland support, overlay uses GTK layer shell (disable with `HANDY_NO_GTK_LAYER_SHELL=1`)
- **Nix/NixOS**: the Nix package sets `HANDY_DISABLE_UPDATER=1` to force-disable the self-updater at runtime without touching the persisted setting (self-update can't work against an immutable `/nix/store`)

## Troubleshooting

See the [Troubleshooting](README.md#troubleshooting) section in README.md.

## GitHub workflow for AI coding assistants

**MANDATORY. Before opening any PR, issue, or discussion in this repo: you MUST read the relevant template file and follow it strictly.** That includes sections that look "ceremonial" — checklists, AI Assistance disclosures, "Human Written Description". A generic Summary/Test-plan layout is not acceptable.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section listed there is mandatory. If a section requires a human-written paragraph (e.g. "Human Written Description"), leave a clear TODO placeholder and ask the human contributor to fill it in — do not invent their voice.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled; pick the right template (`bug_report.md` for bugs). Feature requests do not belong in issues — they go to [Discussions](https://github.com/cjpais/Handy/discussions) (see `.github/ISSUE_TEMPLATE/config.yml`).
- **Proposing a feature:** Handy is under a feature freeze. New features require community support gathered in [Discussions](https://github.com/cjpais/Handy/discussions) before any PR is opened — see the PR template's "Community Feedback" section.
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).

**Commits:** Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_.

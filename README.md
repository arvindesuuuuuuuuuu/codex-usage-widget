# Code Usage Widget — Codex and Cursor

A small Tauri 2 desktop widget for Windows and Linux that displays the same Codex account rate-limit data used by Codex's `/status` view.

The **Codex** tab talks to the local Codex CLI through `codex app-server` and calls `account/rateLimits/read`. The **Cursor** tab reads personal usage through your signed-in Cursor dashboard session. Neither provider requires an API key or imports cookies from your regular browser.

## Cursor personal accounts

1. Select **Cursor** in the desktop widget.
2. If you are already signed in to the Cursor desktop app, the widget automatically reuses that account. **Use Cursor app** explicitly selects it. No second login is needed while its saved session remains valid.
3. If you do not have the Cursor app installed or want to use a separate account, select **Web sign-in**, finish signing in, close that window, and click **Refresh**.

Local account detection opens Cursor's `User/globalStorage/state.vscdb` database read-only and reads only `cursorAuth/accessToken`. It does not read your conversations, access refresh tokens, change the Cursor account, or copy the token into the widget's web profile. It rechecks the local account before using cached usage, so switching accounts in Cursor is reflected on refresh. An expired or rejected local session asks you to reopen Cursor; it never silently displays a different web account.

Default locations are `%APPDATA%/Cursor` on Windows, `$XDG_CONFIG_HOME/Cursor` (or `~/.config/Cursor`) on Linux, and `~/Library/Application Support/Cursor` on macOS. For a portable/custom installation, set `CURSOR_USER_DATA_DIR` to the absolute user-data directory containing the `User` folder before launching the widget.

The widget shows Cursor-model and other-model allowance percentages when returned, the billing-cycle reset, and on-demand spending in USD. Older summaries with a total allowance are also supported. Missing allowances are not shown as zero usage.

Optional web sign-in is retained in a separate WebView profile under the widget's local app-data directory (`cursor-session`). **Clear web sign-in** clears only this profile; it does not sign you out of the Cursor IDE or your normal browser. Session credentials stay in the native backend; they are not returned to the widget page or logged. Requests send the session only to `https://cursor.com/api/usage-summary`, with redirects disabled.

This personal-account integration uses Cursor's **undocumented dashboard endpoint**, not the Teams Admin API. It can break if Cursor changes the endpoint or blocks embedded sign-in. An API-format or sign-in error is displayed instead of fabricated usage. Teams pooled budgets and legacy request-count endpoints are not supported.

Reference: [Cursor usage and limits](https://cursor.com/help/models-and-usage/usage-limits), [CodexBar's Cursor integration notes](https://github.com/steipete/CodexBar/blob/main/docs/cursor.md).

Use the provider buttons to switch accounts' usage displays. Each provider refreshes independently; Cursor failures do not affect Codex. The selected provider is remembered, with automatic refresh every five minutes and a 60-second cache.

## What it shows

- 5-hour Codex allowance, when the account returns a 300-minute window
- Weekly allowance, when the account returns a 10,080-minute window
- Relative reset countdowns (e.g. `Resets in 3h 48m` with local clock time)
- Free rate-limit reset credits badge when available (e.g. `🎁 2 free rate-limit resets available`)
- Color-coded progress meters (green for healthy, amber for warning under 40%, red for critical under 15%)
- Cursor models and other models allowances with accurate decimal-ratio scaling
- Cursor total request counts and bonus breakdown (e.g. `1,952 / 2,000 remaining · 48 used`)
- On-demand spend tracking in USD with limit thresholds
- Plan type tags (e.g. `CHATGPT PLUS`, `CURSOR PRO`)
- System tray icon with Show / Refresh / Quit
- One-click **Compact** button in the header or OS minimize button to switch to the always-on-top status bar
- Automatic refresh every 5 minutes; data is cached for 60 seconds

Codex can occasionally return only one rate-limit window for an account/model. The widget shows exactly what `account/rateLimits/read` returns rather than inventing a missing limit.

## Minimized status bar

Click the **Compact** button in the header or the window **Minimize** button to show a floating status bar at the top of that monitor's work area. It stays above other windows and displays the selected provider's first two usage allowances with color-coded status indicators. Usage continues refreshing every five minutes; loading and errors are shown instead of stale percentages.

- **Non-clickable by default**: Accidental clicks on the status bar do not pop up the dashboard. (You can restore the dashboard via the system tray icon, right-clicking the status bar, or enabling click-to-open in Settings).
- **Hover opacity fade**: When your mouse enters the status bar, its opacity decreases automatically so you can see and interact with tabs or windows directly behind it.
- **Settings customization**: Click the gear (⚙️) icon in the main window header to configure:
  - **Default Opacity**: Slider (20% – 100%) for normal visibility.
  - **Hover Opacity**: Slider (5% – 100%) for transparency when the mouse hovers over it.
  - **Draggable Status Bar**: Checkbox to freely drag and reposition the status bar anywhere on your screen.
  - **Click to Open Dashboard**: Checkbox to show the expand button and restore the main window upon clicking.
  - **Click-Through Status Bar**: Checkbox to let all mouse clicks pass completely through to windows or tabs underneath.

## Requirements

1. Codex CLI installed and signed in with ChatGPT.
2. Node.js + npm.
3. Rust toolchain.
4. Tauri OS prerequisites.

Test Codex first:

```powershell
codex
```

Then run `/status` inside Codex. If that works, the widget should be able to use the same local sign-in.

### Windows

Install Node.js and Rust. Microsoft Edge WebView2 is present on normal Windows 10/11 installations; Tauri may prompt/install it when needed.

### Ubuntu / Debian

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Then install Rust from https://rustup.rs/ and Node.js using your preferred method.

## Run in development

```bash
npm install
npm run tauri dev
```

## Build installers / packages

```bash
npm install
npm run tauri build
```

The build produces:
- Standalone release binary: `src-tauri/target/release/codex-usage-widget`
- Debian package: `src-tauri/target/release/bundle/deb/Codex Usage Widget_0.1.1_amd64.deb`
- RPM package: `src-tauri/target/release/bundle/rpm/Codex Usage Widget-0.1.1-1.x86_64.rpm`

## Installing on Ubuntu

### Option 1: Install Debian package (system-wide)

```bash
sudo apt install ./"src-tauri/target/release/bundle/deb/Codex Usage Widget_0.1.1_amd64.deb"
```
*(or using dpkg: `sudo dpkg -i "src-tauri/target/release/bundle/deb/Codex Usage Widget_0.1.1_amd64.deb"`)*

This installs the binary to `/usr/bin/codex-usage-widget` and sets up the desktop launcher and application icons automatically.

### Option 2: Standalone binary install (user-level, no root required)

Copy the compiled binary to your user bin directory:

```bash
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/512x512/apps
cp src-tauri/target/release/codex-usage-widget ~/.local/bin/
cp src-tauri/icons/icon512.png ~/.local/share/icons/hicolor/512x512/apps/codex-usage-widget.png
```

Create a desktop entry at `~/.local/share/applications/codex-usage-widget.desktop`:

```ini
[Desktop Entry]
Name=Codex Usage Widget
Comment=Codex and Cursor Usage Tracker
Exec=sh -c '"$HOME"/.local/bin/codex-usage-widget'
Icon=codex-usage-widget
Terminal=false
Type=Application
Categories=Utility;Development;
StartupWMClass=codex-usage-widget
```

Then refresh the desktop database:
```bash
update-desktop-database ~/.local/share/applications
```

## How the usage read works

The Rust backend starts:

```text
codex app-server --listen stdio://
```

and performs the JSON-RPC sequence:

```text
initialize
initialized
account/rateLimits/read
```

The response contains fields such as `usedPercent`, `windowDurationMins`, and `resetsAt`. The UI converts `usedPercent` to remaining percentage with `100 - usedPercent`.

## Notes

- On Windows, npm's Codex launcher may be `codex.cmd`, so the backend starts it through `cmd.exe` with a hidden console window.
- On Linux, GUI applications can inherit a different PATH from your shell. If the widget says it cannot start Codex even though `codex` works in your terminal, launch the widget from a terminal first or make sure the Codex install directory is available in your desktop-session PATH.
- The widget intentionally uses Codex's local app-server protocol rather than private ChatGPT HTTP endpoints.

## Important: browser vs Tauri window

Do **not** open the Vite localhost URL in Chrome/Brave/Firefox. The Codex usage reader calls Tauri's native `invoke()` API, which only exists inside the desktop window launched by Tauri.

Use:

```powershell
npm run tauri dev
```

The Vite server will run on `127.0.0.1:1420`, and Tauri will open a separate native **Codex Usage** window. If you manually open the localhost URL in a browser, the page will now show a clear message instead of crashing with `Cannot read properties of undefined (reading 'invoke')`.

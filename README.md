# Mint Launcher

A fast, clean, open-source Minecraft launcher. Manage multiple instances, browse and install
mods/resource packs from Modrinth, sign in with a real Microsoft account or play offline, and
bring your existing setup over from the official launcher, Prism/MultiMC/PolyMC, CurseForge, or
the Modrinth App.

<!-- TODO: add a screenshot or two here -->

## Download

Grab the latest build from the [Releases page](https://github.com/m3l0ncru2t/Mint-Launcher/releases/latest):

- **Windows** - installer (`.exe`), or a portable `.zip` if you'd rather not install anything (keeps
  all its data next to the exe, so the whole folder can live on a USB stick)
- **macOS** - `.dmg`
- **Linux** - `.AppImage` or `.deb`

> **Note:** Mint Launcher isn't code-signed yet, so Windows SmartScreen or your browser may warn
> you about it on first run ("Windows protected your PC"). Click "More info" → "Run anyway". This
> is normal for a new, independently-published app and will go away once signing is set up.

You do **not** need Java installed beforehand - if your system doesn't have a compatible JDK, Mint
Launcher downloads the right one automatically the first time you launch an instance that needs it.

## Features

- **Instances** - each with its own mods, resource packs, saves, and settings; drag to reorder
- **Modrinth integration** - search and install mods/resource packs, check for and apply updates
- **Accounts** - real Microsoft sign-in (Xbox Live/XSTS/Minecraft services) or offline play, with
  support for multiple saved accounts and per-instance account binding
- **Import** - bring in instances from the official launcher, Prism/MultiMC/PolyMC, CurseForge, or
  the Modrinth App, or restore a Mint Launcher backup
- **Automatic Java** - detects the Java version each Minecraft release needs and downloads a
  matching runtime for you if one isn't already available
- **Live console** - streamed game output with a one-click copy button, and automatic crash hints
  for common failure causes
- **Custom themes and backgrounds**
- **Auto-updating** - both the installed and portable builds check for and install updates

## Requirements

- Windows 10+, macOS 11+, or a reasonably modern Linux distro
- Nothing else - Java is handled for you (see above)

## Building from source

### 1. One-time system dependencies (Linux only)

Tauri's webview needs a few dev packages that require root:

```
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config
```

(Not needed on Windows/macOS - Tauri uses the OS's built-in webview there.)

### 2. Run in development

```
npm install
npm run tauri dev
```

### 3. Microsoft account login

Both offline play and real Microsoft account login work out of the box - Mint Launcher ships
with a working Azure AD app registration by default (`msa::DEFAULT_CLIENT_ID` in
`src-tauri/src/msa.rs`). It's actually [Prism Launcher](https://prismlauncher.org/)'s own public
client ID, not one registered for this project: freshly-registered personal Azure apps
consistently got rejected by Minecraft's `login_with_xbox` with a 403 "Invalid app
registration" error, even with every documented setting correct (personal accounts only,
public client flows enabled, matching redirect URI) - Microsoft's Xbox Live sign-in scope
appears to apply extra anti-abuse scrutiny to brand-new apps that no configuration fixes.
Reusing a long-established, known-good client ID (not a secret) sidesteps this, and is common
practice among small/hobby Minecraft launchers for exactly this reason.

If you'd rather use your own app registration (e.g. for your own fork), you can override it in
Settings - just be aware you may hit the same rejection on a brand-new app:

1. Go to [portal.azure.com](https://portal.azure.com) → Azure Active Directory → App registrations → New registration.
2. Name it anything, choose "Personal Microsoft accounts only" as the supported account type.
3. Add a platform: "Mobile and desktop applications" → add redirect URI `http://localhost` (no port - Microsoft's loopback exception matches any port against this).
4. Under Authentication → Advanced settings, enable "Allow public client flows".
5. Copy the "Application (client) ID" from the Overview page and paste it into Mint Launcher's Settings.

### Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Status

Vanilla and Fabric instances are fully playable. Forge and Quilt are modeled in the data layer but
their installers aren't wired up yet.

## License

[GPL-3.0](LICENSE)

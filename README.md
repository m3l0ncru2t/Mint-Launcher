# Mint Launcher

A custom Minecraft launcher built with Tauri (Rust) + React/TypeScript.

## Status

- Instance management (create/list/delete, each with its own game dir, mods folder, natives, memory setting)
- Vanilla version listing + download (client jar, libraries, assets) with SHA1 verification
- Offline accounts (local play) and real Microsoft account login (browser sign-in via OAuth2 authorization code + PKCE)
- Live launch progress + game log streaming to the UI
- Mod loaders (Fabric/Forge/Quilt) - data model is in place, installers not wired up yet

## Setup

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

A Java Development Kit (JDK 17+ for modern Minecraft, JDK 8 for old versions) must be installed and on `PATH` - Mint Launcher does not bundle or auto-download a JVM yet.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

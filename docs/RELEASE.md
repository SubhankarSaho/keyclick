# Release process & code signing

Never commit certificates, private keys, `.p12`/`.pfx` files, App Store Connect keys or passwords.
`.gitignore` blocks the common file types; all secrets below come from environment variables
or CI secret stores.

## 1. Prepare

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.
2. `npm ci`
3. `npm test` — Rust tests + UI type-check must pass.
4. Run the manual checklist in [TESTING.md](TESTING.md) on both platforms.

## 2. Windows (run on Windows 10/11)

Prerequisites: Rust (stable), Node.js 20+, and either the MSVC build tools
(`x86_64-pc-windows-msvc`, recommended for releases) or the GNU toolchain.

```powershell
npm run build-windows
# -> release/KeyClick-Setup.exe   (NSIS, per-user install, no admin needed)
# -> release/KeyClick.msi         (WiX)
```

Tauri downloads NSIS and WiX on first use. Set `KEYCLICK_BUNDLES=nsis` to build only the `.exe`.

### Windows code signing

An unsigned installer triggers SmartScreen ("Windows protected your PC"). To sign:

1. Buy an **OV or EV code-signing certificate** from a CA (DigiCert, Sectigo, SSL.com, …) or use
   **Azure Trusted Signing**. Since 2023 new OV/EV keys must live on an HSM/token or cloud HSM.
2. Option A — certificate in the Windows certificate store: set in `tauri.conf.json` →
   `bundle.windows`:
   ```json
   "certificateThumbprint": "<SHA1 thumbprint>",
   "digestAlgorithm": "sha256",
   "timestampUrl": "http://timestamp.digicert.com"
   ```
   The thumbprint is not secret, but keep it out of shared config if you prefer and pass it
   with `tauri build --config '{"bundle":{"windows":{"certificateThumbprint":"..."}}}'`.
3. Option B — cloud/HSM signing (Azure Trusted Signing, SSL.com eSigner, …): set
   `bundle.windows.signCommand`, e.g.
   `"signCommand": "trusted-signing-cli -e <endpoint> -a <account> -c <profile> %1"`.
   Credentials come from environment variables in CI (`AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`,
   `AZURE_TENANT_ID`).
4. Tauri signs the app binary, the NSIS installer and the MSI.
5. Verify: `signtool verify /pa /v release\KeyClick-Setup.exe`.

SmartScreen reputation builds up over downloads; EV certificates get it immediately.

## 3. macOS (run on macOS with Xcode command line tools)

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run build-macos
# -> release/KeyClick.dmg  (universal: Apple Silicon + Intel)
# -> release/KeyClick.app
```

### Apple code signing & notarization

Requirements:

- **Apple Developer Program** membership (paid, individual or organization).
- A **Developer ID Application** certificate (Xcode → Settings → Accounts → Manage Certificates,
  or developer.apple.com → Certificates). It is installed in the login keychain.
- For notarization: an **App Store Connect API key** (recommended) or an Apple ID with an
  app-specific password.

Signing identity — do not hardcode it; export it before building:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
```

Notarization (Tauri submits and staples automatically when these are set):

```bash
# Preferred: API key
export APPLE_API_ISSUER="<issuer-uuid>"
export APPLE_API_KEY="<key-id>"
export APPLE_API_KEY_PATH="$HOME/.private_keys/AuthKey_<key-id>.p8"
# or: Apple ID
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="<app-specific-password>"
export APPLE_TEAM_ID="TEAMID"

npm run build-macos
```

CI (GitHub Actions etc.): store the `.p12` as base64 in `APPLE_CERTIFICATE` and its password in
`APPLE_CERTIFICATE_PASSWORD`; Tauri imports it into a temporary keychain.

The app uses the **hardened runtime** (`bundle.macOS.hardenedRuntime: true`) and needs no extra
entitlements: a listen-only event tap only requires the user-granted *Input Monitoring*
permission. The app is not sandboxed.

Verify:

```bash
codesign --verify --deep --strict --verbose=2 release/KeyClick.app
spctl --assess --type execute --verbose release/KeyClick.app
xcrun stapler validate release/KeyClick.dmg
```

**Important:** the Input Monitoring grant is tied to the code signature. Ad-hoc/unsigned dev
builds must be re-approved after every rebuild; signed releases keep the grant across updates.

## 4. Publish

Upload `KeyClick-Setup.exe`, `KeyClick.msi` (optional) and `KeyClick.dmg` to the website
(`website/index.html`) together with SHA-256 checksums:

```bash
shasum -a 256 release/*          # macOS
Get-FileHash release\* -Algorithm SHA256   # Windows
```

## 5. Updates (future)

See [ARCHITECTURE.md → Updates](ARCHITECTURE.md#updates). V1 has no auto-updater.

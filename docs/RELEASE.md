# Release process

## Pipelines

- **CI** (`.github/workflows/ci.yml`): on every push/PR on three platforms `cargo fmt --check`,
  `cargo test -p yattat-core`, `cargo clippy`, `npm run check`, `npm run build`, `cargo clippy -p yattat`.
- **Release** (`.github/workflows/release.yml`): on a tag `v*` (or manually)
  `tauri-apps/tauri-action` builds the installers and creates a **release draft**:
  - macOS: universal `.dmg` (Apple Silicon + Intel)
  - Windows: `.msi` (WiX) and `-setup.exe` (NSIS, per-user install)
  - Linux: `.deb`, `.rpm`, `.AppImage`

## Bump the version and publish

1. Set the version in `package.json` (the Tauri configuration reads it from there) and in
   `Cargo.toml` (`[workspace.package] version`); `Cargo.lock` is updated by the next build
   (`cargo check`).
2. Add an entry to `CHANGELOG.md`.
3. Commit, tag and push:

```bash
git tag v0.4.0 && git push origin main --tags
```

4. After the run, check the draft under "Releases" and publish it.

## Signing (optional, recommended)

Without a signature macOS Gatekeeper blocks the first start (right-click → Open or
`xattr -dr com.apple.quarantine`), Windows SmartScreen warns. For signed builds set the following
repository secrets:

| Secret | Meaning |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application certificate as Base64 (`.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password of the `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Company (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | For notarisation (app-specific password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Only needed when the updater plugin is enabled |

Windows code signing can be added through `bundle.windows.certificateThumbprint` in
`src-tauri/tauri.conf.json` and a certificate installed on the runner.

The signing variables are only exported in the workflow when the secrets are set; an empty
`APPLE_CERTIFICATE` would otherwise make the macOS build fail at the signing step.

## Local build

```bash
npm run tauri build
```

The packages end up under `target/release/bundle/`.

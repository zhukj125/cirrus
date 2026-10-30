# Automatic updates

Cirrus uses Tauri's signed updater. The app checks the latest GitHub Release on startup and provides download, install, and restart controls in **Settings > About**.

## Update feed

The updater reads:

```text
https://github.com/zhukj125/cirrus/releases/latest/download/latest.json
```

Release builds must enable updater artifacts and upload `latest.json`. This is configured in `src-tauri/tauri.conf.json` and `.github/workflows/release.yml`.

## Signing key

`src-tauri/tauri.conf.json` contains the public key. The matching private key is not committed and must be stored as a GitHub Actions secret:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of the private key file
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the key password, if one was set

For a passwordless key, only the first secret is required.

On PowerShell, after verifying the local key file:

```powershell
Get-Content -Raw .tmp/cirrus-updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY
```

Never commit the private key or paste it into an issue, pull request, or chat message.

## Release flow

1. Bump the workspace and app version.
2. Commit the version change.
3. Create and push the release tag.
4. The release workflow builds signed updater artifacts and attaches `latest.json`.
5. The publish job turns the draft release into the latest public release.

An installed build only discovers releases that are newer than its own version. The current version can be checked in **Settings > About > Runtime information**.

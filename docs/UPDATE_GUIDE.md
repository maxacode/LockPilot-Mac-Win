# Update Guide

## 1) Frontend update (applies to BOTH Mac + Windows)

1. Edit files in `packages/ui/`
2. Sync into both apps:
   ```bash
   bash scripts/sync-shared-ui.sh
   ```
3. Commit all changed files.
4. Push to `dev` for prerelease testing.
5. Promote to `main` when validated.

## 2) Mac backend-only update

Use when Rust/Tauri/native behavior changes only for macOS.

1. Edit only under `apps/mac/src-tauri/` (or mac-specific config files)
2. Do **not** edit `packages/ui/` unless UI is intended for both platforms.
3. Commit + push to `dev`.
4. Validate mac artifact from CI.

## 3) Windows backend-only update

1. Edit only under `apps/windows/src-tauri/` (or windows-specific config files)
2. Keep shared UI untouched unless this is a cross-platform UI change.
3. Commit + push to `dev`.
4. Validate windows artifact from CI.

## 4) Release channels

- `dev` branch = prerelease channel
- `main` branch = stable channel
- `release-version.txt` controls the next global release tag (auto-bumped by CI)

Keep updater channels aligned in each app config:
- Mac app updater references monorepo release artifacts for macOS
- Windows app updater references monorepo release artifacts for Windows

## 5) Selective release behavior

- UI changes trigger both macOS + Windows release builds.
- mac backend-only changes trigger macOS release build only.
- windows backend-only changes trigger Windows release build only.
- Manual `workflow_dispatch` triggers both platforms.

## 6) Recommended PR labels

- `frontend`
- `backend-mac`
- `backend-windows`
- `release`

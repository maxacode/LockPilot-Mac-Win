# LockPilot Mac/Win Monorepo

Single repo for **shared frontend** + **platform-specific backends**.

## Layout

- `packages/ui/` → single source of truth for frontend UI
- `apps/mac/` → macOS Tauri app/backend
- `apps/windows/` → Windows Tauri app/backend
- `.github/workflows/` → CI/CD for dev/main builds
- `release-version.txt` → global release tag/version counter
- `docs/UPDATE_GUIDE.md` → what to do for frontend/backend-only updates

## Core Rule

If UI changes, update **only** `packages/ui/` and sync into both apps before pushing.

## Quick commands

```bash
# From repo root
bash scripts/sync-shared-ui.sh

# optional verification
bash scripts/check-ui-sync.sh
```

## Branch flow

- `dev` → prerelease/dev builds
- `main` → stable builds

CI builds only the affected platform(s):
- UI change in `packages/ui/` -> build/release macOS + Windows
- mac backend change in `apps/mac/src-tauri/` -> build/release macOS only
- windows backend change in `apps/windows/src-tauri/` -> build/release Windows only

## Existing repos stay intact

This monorepo does **not** delete or alter your legacy repos:
- `maxacode/LockPilotMac`
- `maxacode/LockPilot-Windows`

It provides a unified future workflow.

# Contributing to Cirrus

Thanks for taking a look at Cirrus. Whether you fix a typo, improve the docs, or tackle an edge case in the authentication flow, every PR helps.

## Where to Start

1. Read [README.md](README.md) and [docs/testing/pr-preflight.md](docs/testing/pr-preflight.md) first so you know the layout and the checks.
2. Pick an issue that has a clear scope and that you can verify yourself. Do not rely on the title alone — read the full report, comments, and reproduction steps, and make sure nobody else is on it.
3. Fork the repo, create a branch, and open a PR against `main`.
4. If you are unsure where to start, prefer the parts that can be verified offline: SRP/password derivation, wire DTO mapping, CloudKit schema construction, and coordinate correction are pure logic with real unit tests, so `make cargo-test` / `make test` tells you whether you broke something. The paths that need a real Apple account (authentication, 2FA, photos) have no automated coverage — assess the risk before touching them.

## Development Setup

### Prerequisites

- Node.js >= 22.13.0 (`.nvmrc` pins `22.13.0`; `nvm use` is enough)
- pnpm 10.10.0 (pinned by the `packageManager` field in `package.json`; use Corepack or an equivalent installation)
- Rust >= 1.88.0
- `make`

**Platform requirements:**

- Windows: Visual Studio C++ build tools with the Windows SDK, plus WebView2.
- macOS: Xcode Command Line Tools.
- Linux: `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `libsecret-1-dev`, and `libdbus-1-dev`.

### Run Locally

```bash
git clone <your-fork-url>
cd cirrus
nvm use
pnpm install      # or: make install
make
```

`make` installs dependencies when needed and starts the Tauri desktop dev environment: Vite listens on port 1420 and Tauri's `devUrl` points at `http://localhost:1420`. If the port is taken, the Makefile fails immediately rather than attaching to the wrong process.

Useful shortcuts:

```bash
make dev-web       # frontend dev server only (port 5173 outside Tauri)
make build         # type check + build the frontend bundle
make check         # format + lint + types + frontend tests
make cargo-clippy  # clippy with warnings as errors
```

### Backend Only

`crates/cirrus-core` does not depend on tauri, so it compiles and tests on its own:

```bash
cargo test -p cirrus-core
cargo check -p cirrus-core
```

## Project Layout

| Path | Purpose |
| --- | --- |
| `apps/desktop/src/` | Vue 3 frontend; `lib/api.ts` is the only caller of `invoke` |
| `apps/desktop/src/styles/` | Tailwind entry (`globals.css`) plus the design tokens in `tokens.css` |
| `crates/cirrus-core/` | Shared core logic: SRP cryptography, Apple protocols, session management, outward DTOs; **no tauri dependency** |
| `crates/cirrus-core/src/apple/` | Endpoint constants, header constants, wire DTOs |
| `crates/cirrus-core/src/services/` | Business flows: `auth` / `devices` / `photos` / `media` |
| `src-tauri/` | Tauri shell: entry point, plugins, `generate_handler`, app state, command layer |
| `src-tauri/src/commands/` | The `session` / `devices` / `photos` command groups |
| `scripts/run-check.mjs` | Entry point of `pnpm check`: format / lint / types / tests in parallel |
| `docs/testing/` | Submission and CI verification docs |
| `dist/` | Frontend build output (produced by `outDir` in `apps/desktop/vite.config.ts`) |

## Making Changes

### Branch Naming

Use a short descriptive branch name, for example:

- `docs/pr-preflight-checklist`
- `fix/session-cleanup-race`
- `feat/photos-live-photo-download`

### Scope

Keep PRs focused. A docs-only PR should not include unrelated code changes, and a bug fix should not also refactor nearby modules unless that refactor is required for the fix.

A few boundaries specific to this repository:

- **Do not move Apple's strings out of the constant modules.** New endpoints, headers, and field names belong in `apple/api.rs`, `apple/http.rs`, or the `cloud_kit_schema` module of `services/photos.rs`; business code refers to constants only.
- **Do not bypass `lib/api.ts` on the frontend.** Page components must not call `invoke()` directly.
- **Do not make `cirrus-core` depend on tauri.** It stays independently compilable and testable.
- If you change the CSP in `src-tauri/tauri.conf.json`, explain in the PR description why the relaxation is needed.

### Commits

Write commit messages in plain language with a conventional prefix:

- `docs: add pr preflight checklist`
- `fix(session): expire idle sessions before eviction`
- `feat(photos): support live photo download`

## Tests

Run the checks that match your change:

```bash
make cargo-check
make cargo-test
make cargo-clippy
make check      # the four frontend checks
```

These are **distinct** checks and cannot substitute for one another: passing formatting does not imply passing types, and passing tests does not imply passing lint. For any Rust change, at minimum run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.

Test quality matters more than test count:

- Exercise production functions and assert observable results, state changes, and error branches. Mock external boundaries (HTTP, filesystem, time), not the behavior under test.
- Do not copy the implementation into a test or use source-string matching to pin function names, field names, or template fragments. Those checks break on harmless refactors without proving runtime behavior.
- Extend the existing behavior suite for a regression instead of adding a second wiring snapshot. Use table-driven cases when only the inputs and expected outputs differ.
- The paths that need a real Apple account (SRP sign-in, 2FA issue/verify, CloudKit pagination) cannot be automated — do not pretend otherwise in a test. Extract the offline-verifiable pieces (derivation vectors, DTO mapping, schema construction, pagination cursor assembly, coordinate correction) into pure functions and test those.
- Do not loosen assertions to make a suite pass, and do not write cases that only assert `status == "OK"` — an assertion should say something about why the behavior is correct.

## Documentation

- Repository docs: `README.md` (English), `README.zh-CN.md` (Chinese, the primary document), `CONTRIBUTING.md` / `CONTRIBUTING.zh-CN.md`, `docs/testing/pr-preflight.md`.
- **Keep the two languages in sync.** If you only change one, say in the PR description why the other does not need the change.
- Every path, command, module name, and version in the docs must be verified against the repository. When in doubt, leave it out.

## Pull Requests

1. Push your branch to your fork.
2. Open a PR against this repository's `main`.
3. In the PR description, explain what changed, why, and how you verified it — paste the commands you actually ran and their results.
4. Attach screenshots for UI changes, and state whether an authentication-flow change was verified against a real account.
5. After pushing, verify the remote state as described in [docs/testing/pr-preflight.md](docs/testing/pr-preflight.md). Local results are not a substitute for remote results.

Small PRs are easier to review and merge.

> [!NOTE]
> This repository uses husky. The `pre-commit` hook runs `lint-staged` (which formats staged `apps/desktop/src/**/*.{ts,vue}` with `oxfmt`) and runs `cargo fmt` for staged Rust files under `src-tauri` and `crates/cirrus-core`, then re-stages the formatted output. Run `make check` before committing so the hook cannot rewrite files in a way you did not expect.

## What We Are Looking For

- Documentation improvements and Chinese/English sync
- Interface polish and interaction details (map markers and overlays, the photo grid and full-size viewer, the 2FA flow)
- Offline-reproducible fixes and tests for pure logic (SRP derivation, wire mapping, CloudKit schema, coordinate correction)
- Better error messages and log redaction
- Edge cases around session lifetime, concurrency, and cleanup

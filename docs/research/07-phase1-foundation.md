# Phase 1 foundation research

Research date: **2026-10-15**. Scope: documentation only; no implementation or scaffolding.

This report follows [the Japanese architecture design](../specs/2026-10-15-nexora-architecture-design.md), especially sections 3, 4, 11, and 13. Its requirements remain authoritative: Tauri-independent `nexora-core`, a thin desktop adapter, recoverable task snapshots, Japanese/English UI, SQLite migrations, redacted daily logs, and CI on three operating systems. Windows 11, pnpm, Node 24, and Rust 1.96 are supplied environment assumptions, not independently verified installation facts.

**Evidence policy.** Links below point to official documentation, upstream repositories/releases, or package registries. “Verified” means the retrieved source supports the specific API or version; it does not mean Nexora has been built with it. Recommendations are engineering judgments. **UNVERIFIED** identifies unresolved claims or integration behavior. Registry access through the shell was blocked; several npm/crates.io pages also returned errors or JavaScript-only content. Official release pages and maintainer-authored documentation hosted on docs.rs were used as fallbacks. Search-indexed npm metadata can lag releases; where newer official release evidence exists, it takes precedence. This is an October 15 snapshot, not a claim about releases later in October.

## 1. Tauri layout, workspace, and permissions

### Repository boundaries and launch location

Keep the architecture's root `Cargo.toml`, `crates/nexora-core/`, `src-tauri/`, and `ui/`. `src-tauri` owns the Tauri configuration, build script, capabilities, and command adapter. Keep the frontend package and lockfile in `ui/`; a root JavaScript package is unnecessary. Cargo workspace members share the root lockfile and target directory. A virtual workspace must choose its resolver explicitly. [Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html)

Proposed root manifest excerpt, for the later implementation:

```toml
[workspace]
members = ["crates/nexora-core", "src-tauri"]
default-members = ["crates/nexora-core"]
resolver = "3"
```

Recommendation: use Rust edition 2024 consistently; put shared dependency versions in `[workspace.dependencies]`. The desktop member depends on `nexora-core` by path. `default-members` makes ordinary core development convenient; CI must still use `--workspace` to include the desktop adapter. Keep one workspace `Cargo.lock` under version control.

Run the Tauri CLI from the repository root. Prefer the Cargo CLI (`cargo tauri dev`, from root), or a frontend script explicitly changing to the parent before invoking `tauri`. The latter can expose `pnpm --dir ui run tauri dev`, with the script `cd .. && tauri`. pnpm makes local dependency executables available to scripts. A bare `pnpm --dir ui exec tauri dev` changes the launch directory and must not be assumed equivalent to the root invocation. **UNVERIFIED:** this exact script/CLI discovery combination has not been run in Nexora. Check `tauri info` and hook working directories before adopting it. [pnpm scripts](https://pnpm.io/cli/run), [Tauri CLI reference](https://v2.tauri.app/reference/cli/)

### Frontend build settings

Proposed `src-tauri/tauri.conf.json` build excerpt, assuming root frontend-hook working directory:

```json
{
  "build": {
    "beforeDevCommand": "pnpm --dir ui run dev",
    "beforeBuildCommand": "pnpm --dir ui run build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../ui/dist",
    "additionalWatchFolders": ["../crates/nexora-core"]
  }
}
```

`frontendDist` is relative to the Tauri configuration file, so `../ui/dist` is correct for this layout. Hook objects also accept `cwd` and `script`; the dev hook's `wait` defaults to false. Keep the dev server running without waiting for its exit. **UNVERIFIED:** the composed example's hook/watch paths have not been exercised with the pinned CLI. If launch-directory detection differs, explicitly set the hook `cwd` to the repository root instead of changing the asset path. [Build configuration API](https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.BuildConfig.html)

Configure Vite inside `ui/` with `server.port: 5173`, `strictPort: true`, and output `dist`. Tauri's Vite guide documents the matching fixed dev URL and strict port. Do not copy its root frontend `../dist` path unchanged. [Tauri Vite integration](https://v2.tauri.app/start/frontend/vite/)

### Minimal capabilities

Tauri 2 uses capability files in `src-tauri/capabilities/`, scoped to window/webview labels. Use one local capability for `main`; do not grant remote origins IPC access. Application commands registered with `invoke_handler` are allowed by default unless opted into application ACL through `tauri_build::AppManifest::commands`. There is no general `allow-invoke` permission to add. If opting into application ACL, declare the application command list in `build.rs` and explicit permissions in `src-tauri/permissions/*.toml`. [Capabilities and application commands](https://v2.tauri.app/security/capabilities/)

Minimum proposed capability when frontend code listens for task changes, opens approved HTTPS links, and eventually checks/installs updates:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main",
  "description": "Local main-window IPC",
  "windows": ["main"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    {
      "identifier": "opener:allow-open-url",
      "allow": [{ "url": "https://github.com/*" }]
    },
    "updater:allow-check",
    "updater:allow-download-and-install"
  ]
}
```

The event listener permissions suffice for receiving Rust events; frontend `emit`/`emitTo` need separate grants. `core:default` is convenient but broader. [Core event permission table](https://v2.tauri.app/reference/acl/core-permissions/)

The opener scope above is illustrative: enumerate the actual external destinations before implementation. `opener:default` additionally permits standard URL schemes and reveal-in-folder, so it is broader than opening one approved destination. [Opener scopes and permissions](https://v2.tauri.app/plugin/opener/)

For split update download/install calls, replace the combined grant with `updater:allow-download` and `updater:allow-install`; `updater:default` grants the whole workflow. The architecture schedules actual self-update delivery for phase 10: phase 1 can omit updater grants and initialization until a usable update configuration exists. [Updater permissions](https://v2.tauri.app/plugin/updater/)

Single-instance is Rust-only and needs **no JavaScript capability**. Register it first. Do not invent a `single-instance:default` permission or install a guest package for it. [Single-instance setup and permissions](https://v2.tauri.app/plugin/single-instance/)

Exact verified release-index versions are Tauri **2.12.3**, opener **2.7.1**, single-instance **2.5.3**, and updater **2.13.3**. The architecture's updater **2.14** is **UNVERIFIED and conflicts with the official index**; do not silently turn it into a dependency pin. [Official ecosystem releases](https://tauri.app/release/)

## 2. Windows materials and title bars

Tauri exposes `app.windows[].windowEffects`. Effects require `transparent: true`; Linux effects are unsupported. Windows options include `mica`/`micaLight`/`micaDark` on Windows 11 and `acrylic` on Windows 10/11. Acrylic has documented resizing/dragging performance problems on some builds. Do not treat a list of conflicting effects as a guaranteed Windows fallback chain. [Window effects configuration](https://v2.tauri.app/reference/config/#windoweffectsconfig)

Proposed Windows-only window fragment, merged into a complete `main` window definition:

```json
{
  "label": "main",
  "title": "Nexora",
  "decorations": true,
  "transparent": true,
  "windowEffects": { "effects": ["mica"] }
}
```

Make the webview/document background transparent where the material should appear; opaque CSS surfaces can cover it. Decorations and shadows have platform caveats. Transparency does not itself mean click-through. **UNVERIFIED:** visual quality, compositor behavior, and the exact decorated-window interaction on the user's Windows build. [Window configuration fields](https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html)

macOS equivalents include `sidebar`, `windowBackground`, and `underWindowBackground` visual-effect materials. Use an active-state-following material, preserve native traffic lights, and test inactive-window contrast. Current Tauri configuration documents `macOSPrivateApi` as a no-op in **2.12.1+**; older advice to always enable this flag is version-dependent. Do not assume older Cargo-feature instructions apply to the pinned release. [Effects and macOS flag](https://v2.tauri.app/reference/config/)

Recommendation: **retain native title bars for phase 1**. Mica is an optional visual enhancement, not a reason to replace window controls. Use solid semantic surface colors on Linux and whenever effects are unavailable or disabled. Test high contrast, reduced transparency, light/dark themes, and multiple DPI settings before making materials default.

A custom title bar requires `decorations: false`, explicit drag regions, accessible minimize/maximize/close controls, and the corresponding window command permissions. macOS also offers title bar overlay customization. These APIs do not establish complete parity with native window management. [Tauri window customization](https://v2.tauri.app/learn/window-customization/)

Recommendation if custom chrome is revisited: require keyboard operation, screen-reader labels, resize edges, double-click maximize, system menu, focus indication, and Windows Snap behavior as acceptance checks. **UNVERIFIED:** native Snap-layout hover parity for an HTML maximize button; do not promise it based on a CSS imitation.

## 3. Tailwind CSS v4 and React Aria Components

Use `tailwindcss` and `@tailwindcss/vite`, both **4.3.3**, with `tailwindcss()` alongside the React plugin in `ui/vite.config.ts`. Import `@import "tailwindcss";` in the application stylesheet. The official Vite integration uses this plugin; a v3-style Tailwind initialization/configuration workflow is unnecessary for the baseline. [Vite installation](https://tailwindcss.com/docs/installation/using-vite), [Tailwind registry](https://www.npmjs.com/package/tailwindcss?activeTab=versions), [Vite adapter registry](https://www.npmjs.com/package/%40tailwindcss/vite?activeTab=versions)

Put design tokens in top-level `@theme`; use `@theme inline` when utility tokens reference ordinary CSS variables whose values change with the theme. Ordinary `:root` variables alone do not define Tailwind utilities. [Theme variables](https://tailwindcss.com/docs/theme)

Proposed minimal CSS policy (values illustrate mechanics, not a finalized visual design):

```css
@import "tailwindcss";
@plugin "tailwindcss-react-aria-components";
@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *));

:root { --surface: #ffffff; --ink: #18181b; color-scheme: light; }
:root[data-theme="dark"] {
  --surface: #18181b; --ink: #fafafa; color-scheme: dark;
}
@theme inline {
  --color-surface: var(--surface);
  --color-ink: var(--ink);
}
```

Persist `system | light | dark` as the user's preference. Resolve `system` through `matchMedia('(prefers-color-scheme: dark)')`, write the resolved `light`/`dark` value to `document.documentElement.dataset.theme`, and listen for system changes only while following the system. Apply the initial value before React paints. Overriding `dark` to an attribute selector replaces its default media-query behavior; the resolver must reconnect system preference explicitly. [Dark mode and system preference](https://tailwindcss.com/docs/dark-mode)

React Aria Components **1.22.1** has an official October 9 release; retrieved npm search metadata still shows **1.21.1**, so that cached result is not the best current release evidence. **UNVERIFIED:** a fresh npm dist-tag response for 1.22.1 could not be retrieved. The upstream release and current package manifest agree on 1.22.1. [Official release](https://github.com/adobe/react-spectrum/releases/tag/react-aria-components%401.22.1), [Upstream package manifest](https://github.com/adobe/react-spectrum/blob/main/packages/react-aria-components/package.json)

`tailwindcss-react-aria-components` **2.2.0** explicitly supports Tailwind v4; its manifest has peer dependency `tailwindcss: ^4.0.0`. Load it using CSS `@plugin`, as above. Adobe says v3 consumers should use plugin 1.x. The plugin is optional: `data-[selected]:…`, `data-[disabled]:…`, and render-prop class functions also work. Recommendation: use 2.2.0 for concise state variants, while documenting the variants' behavior on native controls. [Adobe styling guide](https://react-aria.adobe.com/styling), [Plugin manifest](https://github.com/adobe/react-spectrum/blob/main/packages/tailwindcss-react-aria-components/package.json), [Plugin registry](https://www.npmjs.com/package/tailwindcss-react-aria-components)

## 4. Japanese/English i18n and TypeScript keys

Recommendation: bundle `ui/src/i18n/{ja,en}/{common,errors,tasks,settings}.json` with static imports. Initialize one i18next instance through `initReactI18next` before mounting React. Use `resources: { ja: {...}, en: {...} }`, `supportedLngs: ['ja', 'en']`, `fallbackLng: 'en'`, `defaultNS: 'common'`, explicit namespace names, and `interpolation.escapeValue: false` for React-rendered text. Use `useTranslation('tasks')` at feature boundaries. Avoid network translation backends for this offline launcher. [React i18next initialization](https://react.i18next.com/latest/using-with-hooks)

Language precedence recommendation: saved explicit choice, then `await locale()` from `@tauri-apps/plugin-os`, then `navigator.language`, then English. The plugin returns a BCP-47 tag or null; the browser property reports the browser/user preferred language and is not a guarantee of the OS locale. Map `ja-*` to `ja`, `en-*` to `en`, and unsupported tags to the fallback. For browser-only development, skip native detection. [Tauri locale API](https://v2.tauri.app/reference/javascript/os/#locale), [Navigator language](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/language)

Enable the OS plugin in Rust if used and grant only its locale command permission (`os:allow-locale`). **UNVERIFIED:** the permission identifier must be checked against the generated schema for the pinned plugin; its locale API is verified, but that permission's generated definition was not retrieved. If using only `navigator.language`, the plugin and grant are unnecessary.

For type-safe keys, enable TS `strict` and `resolveJsonModule`, include an `i18next.d.ts` augmentation, and use English resource imports as the canonical key shape:

```ts
import 'i18next';
import common from './en/common.json';
import errors from './en/errors.json';
import tasks from './en/tasks.json';
import settings from './en/settings.json';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'common';
    resources: {
      common: typeof common;
      errors: typeof errors;
      tasks: typeof tasks;
      settings: typeof settings;
    };
  }
}
```

This checks keys, not translation completeness or every interpolation parameter. JSON does not provide `as const` literal strings for full interpolation inference; generate resource declarations if that becomes necessary. Keep a key-set parity check for Japanese and English. i18next also offers selector APIs, but they are not required for this small namespace set. [Official TypeScript guide](https://www.i18next.com/overview/typescript)

Recommendation: represent error codes as a generated finite union and translate within the `errors` namespace, e.g. `t(code, { ns: 'errors', ...params })`. This is equivalent to the architecture's conceptual `errors.<code>` key. Handle unknown future codes with a generic localized error rather than asserting arbitrary backend strings are valid translation keys.

Verified release evidence: i18next **26.4.5**, released October 14, supersedes stale npm metadata for 26.4.3. react-i18next registry metadata reports **17.0.15**. **UNVERIFIED:** the latter's exact latest status on October 15, because that search snapshot is older. [i18next releases](https://github.com/i18next/i18next/releases), [react-i18next registry](https://www.npmjs.com/package/react-i18next?activeTab=versions)

## 5. ts-rs 12 and the IPC integer contract

Verified documented version: **ts-rs 12.0.1**. `#[derive(TS)]` plus `#[ts(export)]` generates export tests executed by `cargo test`; `cargo test export_bindings` filters to binding exports. Default output is `./bindings`; `TS_RS_EXPORT_DIR` changes it. Set a single absolute export directory in CI to avoid different crate working directories. For PowerShell, resolve the existing destination parent and assign `$env:TS_RS_EXPORT_DIR` to the full `ui/src/bindings` path before tests; use the equivalent absolute environment value on Unix. [ts-rs configuration](https://docs.rs/ts-rs/latest/ts_rs/)

Recommendation for later persistent configuration: root `.cargo/config.toml` can contain `TS_RS_EXPORT_DIR = { value = "ui/src/bindings", relative = true }` under `[env]`. Keep `export_to` paths relative to this shared base. Export public command, error, and task DTOs; avoid exporting database internals. This report does not create that configuration. [Export and path attributes](https://docs.rs/ts-rs/latest/ts_rs/trait.TS.html)

The default `serde-compat` feature understands `rename_all`, `rename_all_fields`, `tag`, `content`, and `untagged`, among other attributes. Prefer consistently renamed structs and explicitly tagged enum DTOs; keep unsupported-Serde warnings enabled. A useful task-state representation is `#[serde(tag = "state", content = "data", rename_all = "snake_case")]`. Verify actual Serde JSON fixtures against TypeScript consumers, especially optional fields and custom serialization. [Serde compatibility](https://docs.rs/ts-rs/latest/ts_rs/)

**Critical distinction:** `TS_RS_LARGE_INT` defaults to `bigint` for `i64/u64/i128/u128`; setting it to `number` changes generated declarations, not Serde JSON encoding or JavaScript parsing. A type assertion does not convert an IPC number into a BigInt. [Large integer configuration](https://docs.rs/ts-rs/latest/ts_rs/)

Recommendation: choose **`TS_RS_LARGE_INT=number` for phase 1 IPC DTOs**, with explicit safe-integer validation at the boundary. Millisecond timestamps and practical launcher progress counters fit this policy; arbitrary 64-bit values do not. For values requiring the entire integer range, serialize decimal strings and expose TS `string`, then explicitly parse BigInt only where needed. Never silently narrow a large integer to a double. **UNVERIFIED:** a lossless native BigInt transport across every relevant Tauri IPC path; do not rely on it.

Generated types do not validate runtime payloads. Proposed CI ordering: generate bindings, check tracked generated files for differences and unexpected untracked files, then typecheck/test the frontend. Commit generated bindings for normal frontend development, and make regeneration deterministic on all three OSes. **UNVERIFIED:** exact output normalization and concurrent export behavior with Nexora's future DTO graph; verify once the types exist.

## 6. Commands, structured errors, async tasks, and progress

A command can return `Result<T, CommandError>` where both success and error values serialize. The JavaScript `invoke<T>()` promise rejects with the serialized error value. Use an owned DTO with `code`, `params`, and nullable `detail`, deriving `Serialize` and `TS`; map core `thiserror` variants explicitly at the adapter boundary. Do not assume `std::error::Error` automatically serializes or that TypeScript's generic return type types rejection values. [Tauri errors and async commands](https://v2.tauri.app/develop/calling-rust/)

Recommended wire contract:

```text
CommandError = {
  code: stable finite error-code union,
  params: map of approved string/number/boolean values,
  detail: redacted diagnostic string | null
}
```

Validate caught `unknown` errors in the handwritten `ui/src/api/` wrappers. Keep localization in the UI. Permit a sanitized diagnostic identifier in `detail`; keep tokens, full request bodies, and raw authentication URLs out of every field.

Tauri provides a Tokio-based async runtime and spawn helpers. Use the existing runtime for desktop command/background execution; keep core async functions runtime-agnostic at the Tauri boundary and use `#[tokio::test]` for isolated core tests. Do not construct a nested Tokio runtime in a command. [Tauri async runtime](https://docs.rs/tauri/latest/tauri/async_runtime/index.html)

Recommendation: starting a long operation allocates/stores the queued task, spawns its worker, and immediately returns `task_id`. Keep owned state/handles in spawned work. Short state locks may guard transitions, but release them before I/O or `.await`. Add cancellation tokens and retain bounded terminal history in memory, matching architecture section 3.5. Use per-instance serialization for mutation tasks; task cancellation must distinguish request acknowledgement (`cancelling`) from worker termination (`cancelled`).

From background work, clone `AppHandle` into the desktop adapter and import `tauri::Emitter`; `app.emit("task://changed", payload)` broadcasts a serializable, cloneable payload. Keep the core connected through an internal notification abstraction; core must not call Tauri directly. Handle failed delivery without failing an otherwise successful task. [Emitter API](https://docs.rs/tauri/latest/tauri/trait.Emitter.html)

| Mechanism | Official properties | Nexora implications / recommendation |
|---|---|---|
| Global events | Simple multi-producer/multi-consumer notifications; JSON payloads; not designed for high throughput or low latency | Fits app-wide task invalidation and a task tray. Throttle progress and recover from snapshots. |
| `tauri::ipc::Channel<T>` | Frontend supplies a channel to a command; Rust calls `send`; optimized for ordered streaming | Useful for high-rate logs or an operation-specific stream. Recreate subscriptions after reload; channel delivery is not persisted task state. |

The API comparison is documented by Tauri; the lifetime/recovery policy is a Nexora design recommendation. Event permission grants do not provide fine-grained per-event payload authorization. [Events and channels](https://v2.tauri.app/develop/calling-frontend/)

**Recommend global events for phase 1 task progress**, preserving the architecture's existing contract. Proposed payload: `taskId`, a monotonic snapshot revision, and a small change hint; do not stream each downloaded chunk. Coalesce nonterminal updates (initial target: at most 10 per second per task; tune from measurement), publish terminal transitions promptly, and let `get_task`/`list_tasks` supply authoritative snapshots. Subscribe before fetching the initial snapshot; discard older revisions and refresh after reload or detected gaps. Reserve Channels for demonstrated stream throughput needs.

## 7. rusqlite in an asynchronous application

Verified versions: **rusqlite 0.40.2** and **rusqlite_migration 2.6.0**. The migration package declares `rusqlite ^0.40.0`, so these direct versions are compatible at the manifest level. `bundled` builds SQLite instead of depending on the user's installed SQLite. **UNVERIFIED:** full transitive dependency resolution/build on Rust 1.96; no Cargo resolution was performed in this documentation task. [rusqlite API](https://docs.rs/rusqlite/latest/rusqlite/), [Migration dependency metadata](https://docs.rs/crate/rusqlite_migration/latest), [Bundled SQLite feature](https://github.com/rusqlite/rusqlite)

`rusqlite::Connection` is `Send` but not `Sync`; statements and transactions are synchronous. [Connection traits](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html)

| Pattern | Consequences | Phase 1 decision |
|---|---|---|
| `Arc<std::sync::Mutex<Connection>>` plus bounded `spawn_blocking` work | One connection and serialized DB work; lock acquisition and queries both happen inside the blocking closure | **Recommended** for the small local settings/metadata database |
| r2d2 + r2d2_sqlite | Multiple synchronous connections; pool checkout/query work still requires a blocking boundary | Defer until concurrent reads justify it |
| deadpool-sqlite | Async pool acquisition and `interact` for connection work on a separate thread | Reasonable later alternative; avoid a second connection abstraction now |

r2d2_sqlite provides the synchronous pool manager; deadpool explicitly wraps threaded interaction. The choice above is a workload judgment, not a claim that pooling increases SQLite write concurrency. [r2d2_sqlite](https://docs.rs/r2d2_sqlite/latest/r2d2_sqlite/), [deadpool-sqlite](https://docs.rs/deadpool-sqlite/latest/deadpool_sqlite/)

Move an owned connection-handle clone into `spawn_blocking`, then lock it, execute the entire transaction, collect owned results, and release it there. Never hold a DB mutex guard across `.await` or block an async worker while waiting for that mutex. Bound queued DB operations so many tasks cannot exhaust blocking workers. `spawn_blocking` work cannot normally be aborted once running; cancellation must be cooperative and transaction boundaries deliberate. [Tokio blocking guidance](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)

At startup, configure `PRAGMA journal_mode=WAL`, enable foreign keys on the connection, and select an explicit busy timeout. Recommendation: start with a 5-second busy timeout and retain SQLite's durability-oriented synchronization behavior until profiling justifies a change. WAL permits readers alongside a writer but still only one writer; it adds `-wal`/`-shm` files and should stay on a local filesystem. Do not back up an active database by copying only its main file. [SQLite WAL](https://sqlite.org/wal.html), [rusqlite connection configuration](https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html)

Embed ordered SQL files with `include_str!`, e.g. `M::up(include_str!("migrations/0001_initial.sql"))`, build a `Migrations` list, and run `to_latest(&mut connection)` before exposing normal DB operations. The crate supports embedded SQL strings, `include_str!`, validation, and an optional directory feature. It uses SQLite `user_version`; do not also manage that field independently. [Migration API](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/)

Recommendation: keep published migrations immutable and append new ones. Test fresh initialization, upgrading a previous schema with data, foreign-key enforcement, and rollback on migration failure using a real temporary on-disk database as well as fast in-memory tests. Open/migrate through the blocking boundary; report startup migration failures as structured errors instead of continuing with a partly initialized service.

## 8. tracing, rolling files, frontend forwarding, and redaction

Use `tracing` in core and install one subscriber in the desktop application. Compose a file formatting layer, development console layer, and a small frontend forwarding layer. Subscriber layers are composable; libraries should emit instrumentation without installing global subscribers. [tracing usage](https://docs.rs/tracing/latest/tracing/), [Subscriber layers](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)

`tracing_appender::rolling::daily(log_dir, "nexora.log")` provides daily rotation. Wrap it in `tracing_appender::non_blocking`, and keep the returned `WorkerGuard` alive for the app lifetime so buffered records flush on orderly shutdown. Do not bind the guard to `_` and immediately drop it. Define retention explicitly; rotation alone is not a disk-usage policy. [Appender and guard](https://docs.rs/tracing-appender/latest/tracing_appender/)

Recommendation: a custom `tracing_subscriber::Layer` visits fields, constructs an allowlisted `UiLogRecord`, and enqueues it into a bounded nonblocking buffer. An adapter task batches sanitized records to the main UI, with a bounded in-memory tail available on reconnect. Emit only after leaving the subscriber callback; never recursively trace emission failures from the same layer. This is proposed application code, **UNVERIFIED as an integrated implementation**. Its building blocks are the subscriber Layer and Tauri Emitter APIs. [Layer API](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/trait.Layer.html), [Emitter API](https://docs.rs/tauri/latest/tauri/trait.Emitter.html)

Tauri's log plugin offers frontend log integration, but it uses its own logging integration and should not be assumed to forward an arbitrary existing tracing subscriber. Recommendation: keep tracing-appender as the single file pipeline and avoid adding another file logger merely for frontend display. A custom bounded forwarding layer avoids duplicate initialization and divergent redaction paths. [Tauri logging plugin](https://v2.tauri.app/plugin/logging/)

Redaction policy recommendation:

- Treat access/refresh tokens, authorization headers, cookies, authentication query parameters, credentials, and full game launch arguments as forbidden fields. Prefer approved operational fields such as task ID, stage, duration, and error code.
- Put secret values in `secrecy` wrappers; expose them only at the request boundary. This reduces accidental disclosure but does not sanitize strings copied out of the wrapper. [secrecy API](https://docs.rs/secrecy/latest/secrecy/)
- Use `#[instrument(skip_all, fields(task_id = ...))]` or explicit `skip` lists around secret-bearing functions. Default instrumentation records arguments through `Debug`. [tracing instrumentation](https://docs.rs/tracing/latest/tracing/)
- Sanitize before both file and frontend sinks. Prefer removal/allowlisting over regex replacement alone. Do not log raw response bodies or URLs with query strings. Redact personal paths in exported diagnostics where feasible.
- Add sentinel-secret tests covering logs, command errors, task failures, and frontend records. Make diagnostic exports opt-in and previewable. These are phase 1 acceptance requirements, not claims that redaction already exists.

## 9. GitHub Actions across Windows, Linux, and macOS

The current upstream README uses **`tauri-apps/tauri-action@v1`**, not the historical `@v0`. Its `projectPath` is the Tauri project root, so use the repository root for this layout; if the npm CLI lives in `ui/`, explicitly supply the tested CLI wrapper rather than relying on root package-manager auto-detection. The action can build without creating a release; omit release inputs in phase 1. [Tauri action](https://github.com/tauri-apps/tauri-action)

For Ubuntu, the action's Tauri 2 example installs:

```text
libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf xdg-utils
```

Install build tooling as needed (`build-essential`, `pkg-config`, `curl`, `wget`, `file`, `libssl-dev`), and use the prerequisites matching the chosen Ubuntu image. Do not substitute WebKitGTK 4.0 from Tauri 1 instructions. **UNVERIFIED:** the minimal package closure for a specific future runner image; the list above is verified as the action's documented example, not a fresh apt installation. [Action Linux dependency example](https://github.com/tauri-apps/tauri-action), [Platform prerequisites](https://v2.tauri.app/start/prerequisites/)

Recommended CI matrix: `windows-latest`, `ubuntu-24.04`, `macos-latest`, with `fail-fast: false`. Pin Node to major 24 and Rust to 1.96.0 for the initial contract, plus a fixed pnpm version chosen from the developer environment. **UNVERIFIED:** availability of Rust 1.96.0 and exact SDK/runner contents were not independently installed/tested; the requested Rust 1.96 environment is an input. Hosted Windows is not proof of Windows 11 desktop rendering, and one macOS runner architecture is not proof of both ARM64 and x86_64 distributions.

Recommended ordered steps on **each** OS:

1. Check out; install platform prerequisites, Node, pinned pnpm, and the Rust toolchain with fmt/clippy.
2. Restore pnpm's store keyed by `ui/pnpm-lock.yaml`; run `pnpm --dir ui install --frozen-lockfile`.
3. Restore Rust cache for the **root** workspace (`. -> target`), then run formatting/lint checks.
4. Set the absolute `TS_RS_EXPORT_DIR` and common `TS_RS_LARGE_INT=number` environment values. Run `cargo test --workspace --locked`. If the desktop build script requires frontend assets for tests, build `ui/` first; generated bindings must already be committed for that bootstrap build.
5. Verify bindings match the checked-in directory, including new/untracked generated files; `git diff --exit-code` alone misses untracked files.
6. Run `pnpm --dir ui run typecheck` and `pnpm --dir ui test -- --run` (define `test` as Vitest). Keep noninteractive UI unit tests on all OSes.
7. Build `ui/` and the desktop target with the tested Tauri CLI/action. An unsigned `--no-bundle` build is sufficient for phase 1 build verification; packaging/signing/updater releases belong to later release work.

Use **`Swatinem/rust-cache@v2`** after selecting the toolchain. Its default workspace mapping is `. -> target`; `src-tauri -> target` would be the wrong assumption for this root Cargo workspace. It keys on compiler/dependency information and benefits from a committed lockfile. [Rust cache documentation](https://github.com/Swatinem/rust-cache)

Node setup supports pnpm caching with an explicit dependency-lockfile path; ensure pnpm is available before cache discovery. Avoid two independent pnpm store cache owners. Pin third-party action commits when implementing the workflow. **UNVERIFIED:** exact October 15 commit SHAs and newest majors for checkout/setup-node/pnpm setup were not established; no guessed SHAs are provided. [Node setup](https://github.com/actions/setup-node), [pnpm setup](https://github.com/pnpm/action-setup)

These commands are a proposed CI contract, not a passing workflow. **UNVERIFIED:** generated scripts, headless desktop build requirements, feature combinations, and all three OS executions. No workflow file or package manifest was created for this research.

## 10. Single-instance, deep links, identifiers, and storage

Recommendation: initialize single-instance before any DB-owning services, then focus/show the existing `main` window in its callback. Register the plugin first. On Linux it uses a D-Bus identity derived from the app identifier; sandboxed packaging needs separate D-Bus declarations. Single-instance does not replace the core's per-instance mutation locks. [Single-instance lifecycle](https://v2.tauri.app/plugin/single-instance/)

Deep-link support uses a separate plugin. On Windows/Linux, a link can start a new process with the URL as an argument; enable single-instance's `deep-link` Cargo feature to forward it into the existing app. Configure desktop schemes under `plugins.deep-link.desktop.schemes`. Handle both cold-start `get_current`/`getCurrent` and warm `on_open_url`/`onOpenUrl` delivery. [Deep linking](https://v2.tauri.app/plugin/deep-linking/)

Recommendation: establish the registration/validation interface in phase 1, but defer real auth callback handling to phase 3. Parse allowlisted schemes/routes in Rust, queue cold-start intents until services/UI are ready, deduplicate delivery, and reject malformed payloads. A deep link is untrusted input; never treat receipt as consent to install or execute. Keep auth-bearing raw URLs out of JS events and logs. OS protocol registration and installed-bundle delivery need platform smoke tests; **UNVERIFIED** until tested with an installed app. Development registration differs from packaged registration. [Desktop registration details](https://v2.tauri.app/plugin/deep-linking/)

Use a stable reverse-domain identifier such as `io.github.<owner>.nexora`, replacing `<owner>` with a real normalized owner segment before implementation. Tauri uses this identity for bundling; choose it before distribution and do not change it for branding edits. The example is a naming recommendation, not a verified identifier owned by this project. [Identifier contract](https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.Config.html)

Tauri's `app_data_dir` is based on the platform data directory plus the configured bundle identifier. Thus changing the identifier can point Tauri-managed storage at a different directory. This is separate from Nexora's `directories`-based core storage; changing Tauri configuration does not automatically migrate the core database. [Tauri path resolver](https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html)

**Architecture discrepancy to resolve in the phase 1 spec:** `directories::ProjectDirs::from(qualifier, organization, application).data_dir()` does not universally produce the simplified paths shown in architecture section 3.4. Its Windows data path includes a `data` suffix; macOS incorporates qualifier/organization/application; Linux derives a normalized project name and respects XDG settings. Do not equate it with `%APPDATA%\Nexora` on the basis of the crate name alone. [ProjectDirs path formulas](https://docs.rs/directories/latest/directories/struct.ProjectDirs.html)

Recommendation: centralize a core `AppPaths` contract. To preserve the architecture's literal `Nexora/` tree, derive the OS base data directory and append the intended product segment explicitly, respecting XDG on Linux; alternatively approve ProjectDirs' actual platform paths in the phase 1 spec. Keep the choice explicit and tested. Use separate development storage as well as a development app identifier; an identifier suffix alone does not isolate a separately configured core data root. **UNVERIFIED:** exact adopted paths remain a phase 1 specification decision; this research does not amend the architecture.

## Implementation checklist

All items are future implementation acceptance criteria; this research completes none of the product setup.

- [ ] Resolve updater version evidence, npm snapshot lag, toolchain availability, and the core data-path discrepancy before committing manifests.
- [ ] Freeze a production identifier, development identifier/data root, and explicit cross-platform `AppPaths` behavior.
- [ ] Create the root Cargo workspace with core and desktop members; keep Tauri out of core and commit one Cargo lockfile.
- [ ] Create `ui/` with the chosen exact frontend versions and pnpm lockfile; verify root CLI discovery, hook cwd, fixed dev port, and `../ui/dist` packaging on Windows.
- [ ] Limit capabilities to the local `main` window and actual API calls; validate scopes/generated permission names. Keep updater permissions disabled until needed.
- [ ] Keep native title bars; verify optional Mica, macOS material, opaque Linux fallback, contrast, and DPI behavior on real systems.
- [ ] Define semantic Tailwind tokens and explicit system/light/dark handling; use RAC keyboard/focus states and plugin 2.x if selected.
- [ ] Bundle ja/en namespaces, typed translation keys, fallback/detection precedence, and translation key parity checks.
- [ ] Generate public DTO bindings during Cargo tests with one absolute destination; document safe integers versus string-encoded full-range integers.
- [ ] Implement command error mapping, runtime validation in UI wrappers, immediate task IDs, snapshot queries, revisions, throttled events, cooperative cancellation, and bounded task history.
- [ ] Open bundled SQLite through a blocking boundary; configure WAL/foreign keys/busy timeout, embed numbered SQL, and validate fresh/upgrade/failure paths.
- [ ] Configure one tracing subscriber, daily nonblocking files, live guard ownership, bounded sanitized frontend forwarding, retention, and sentinel-secret tests.
- [ ] Install single-instance first and test second-launch focus; establish validated cold/warm deep-link handling without implementing auth prematurely.
- [ ] Run Rust tests, frontend tests/typechecking, binding consistency checks, and desktop builds on Windows/Linux/macOS; keep signing and publishing out of the phase 1 CI job.

## Exact dependency versions and verification status

These are **direct-dependency research pins**, not a resolved lockfile. A verified version is not proof that no later release exists. Read the status column for stale registry results or upstream-only release evidence. Optional dependencies need not be installed in phase 1. No transitive versions are invented.

### Rust

| Dependency | Exact version | Features / role and evidence |
|---|---|---|
| `tauri` | **2.12.3** | Desktop member; [official release index](https://tauri.app/release/) |
| `tauri-build` | **2.7.2** | Desktop build dependency; [official release index](https://tauri.app/release/) |
| `tauri-cli` | **2.12.2** | Optional developer tool instead of npm CLI; [official release index](https://tauri.app/release/) |
| `tauri-plugin-opener` | **2.7.1** | Rust plugin; [official release index](https://tauri.app/release/) |
| `tauri-plugin-single-instance` | **2.5.3** | Add `deep-link` only when integrating deep links; [official release index](https://tauri.app/release/) |
| `tauri-plugin-updater` | **2.13.3** | Optional until update work; architecture's 2.14 UNVERIFIED; [official index](https://tauri.app/release/) |
| `tauri-plugin-deep-link` | **2.6.1** | Optional; [official release index](https://tauri.app/release/) |
| `tauri-plugin-os` | **2.4.0** | Optional native locale detection; [official release index](https://tauri.app/release/) |
| `ts-rs` | **12.0.1** | Default `serde-compat`; add `serde-json-impl` only if DTOs need it; [maintainer docs](https://docs.rs/ts-rs/latest/ts_rs/) |
| `rusqlite` | **0.40.2** | `bundled`; [maintainer docs](https://docs.rs/rusqlite/latest/rusqlite/) |
| `rusqlite_migration` | **2.6.0** | Plain `include_str!` needs no directory feature; [package/dependency metadata](https://docs.rs/crate/rusqlite_migration/latest) |
| `tokio` | **1.53.3** | Select `rt-multi-thread`, `macros`, `sync`, `time` as required; [maintainer docs](https://docs.rs/tokio/latest/tokio/) |
| `tokio-util` | **0.7.20** | `rt` for cancellation support; [maintainer docs](https://docs.rs/tokio-util/latest/tokio_util/), [CancellationToken API](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) |
| `serde` | **1.0.229** | `derive`; [maintainer docs](https://docs.rs/serde/latest/serde/) |
| `serde_json` | **1.0.154** | JSON boundary; [maintainer docs](https://docs.rs/serde_json/latest/serde_json/) |
| `thiserror` | **2.0.21** | Core domain errors; [maintainer docs](https://docs.rs/thiserror/latest/thiserror/) |
| `tracing` | **0.1.44** | Instrumentation; [maintainer docs](https://docs.rs/tracing/latest/tracing/) |
| `tracing-subscriber` | **0.3.23** | `env-filter`, `fmt`; optionally `json`; [maintainer docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) |
| `tracing-appender` | **0.2.5** | Daily files/nonblocking writer; [maintainer docs](https://docs.rs/tracing-appender/latest/tracing_appender/) |
| `secrecy` | **0.10.3** | Secret wrappers; [maintainer docs](https://docs.rs/secrecy/latest/secrecy/) |
| `directories` | **6.0.0** | Resolve path-policy choice first; [maintainer docs](https://docs.rs/directories/latest/directories/) |

Pooling is intentionally not part of the recommended direct dependency set. `tempfile`, `insta`, and other optional test utilities: exact current pins **UNVERIFIED** in this pass; do not infer them from the architecture's major-only entries. No log plugin is required for the proposed custom tracing bridge.

### Frontend

| Dependency | Exact version observed / proposed pin | Evidence and qualification |
|---|---|---|
| `react`, `react-dom` | **19.3.1** each | October 14 upstream release supersedes cached npm 19.3.0; **UNVERIFIED fresh npm dist-tags**; [official React release](https://github.com/react/react/releases/tag/v19.3.1) |
| `typescript` | **7.0.2** | Registry latest observed; [npm versions](https://www.npmjs.com/package/typescript?activeTab=versions) |
| `vite` | **8.3.4** | Registry version observed; [npm versions](https://www.npmjs.com/package/vite?activeTab=versions) |
| `@vitejs/plugin-react` | **6.1.2** | Registry version observed; [npm versions](https://www.npmjs.com/package/%40vitejs/plugin-react?activeTab=versions) |
| `tailwindcss` | **4.3.3** | Registry latest observed; [npm versions](https://www.npmjs.com/package/tailwindcss?activeTab=versions) |
| `@tailwindcss/vite` | **4.3.3** | Registry latest observed; [npm versions](https://www.npmjs.com/package/%40tailwindcss/vite?activeTab=versions) |
| `react-aria-components` | **1.22.1** | Official release October 9 and manifest verified; **UNVERIFIED fresh npm dist-tag**, cached registry shows 1.21.1; [upstream release](https://github.com/adobe/react-spectrum/releases/tag/react-aria-components%401.22.1) |
| `tailwindcss-react-aria-components` | **2.2.0** | Optional, verified v4 peer range; [official manifest](https://github.com/adobe/react-spectrum/blob/main/packages/tailwindcss-react-aria-components/package.json) |
| `i18next` | **26.4.5** | Official release October 14; npm snapshot lag documented above; [upstream releases](https://github.com/i18next/i18next/releases) |
| `react-i18next` | **17.0.15** | Registry version verified; **UNVERIFIED latest as of October 15**; [npm versions](https://www.npmjs.com/package/react-i18next?activeTab=versions) |
| `@tauri-apps/api` | **2.12.3** | [Official release index](https://tauri.app/release/) |
| `@tauri-apps/cli` | **2.12.2** | Developer dependency if using frontend CLI wrapper; [official release index](https://tauri.app/release/) |
| `@tauri-apps/plugin-opener` | **2.7.1** | [Official release index](https://tauri.app/release/) |
| `@tauri-apps/plugin-updater` | **2.13.3** | Optional; [official release index](https://tauri.app/release/) |
| `@tauri-apps/plugin-deep-link` | **2.6.1** | Optional when handling guest-side links; Rust-only handling can avoid it; [official release index](https://tauri.app/release/) |
| `@tauri-apps/plugin-os` | **2.4.0** | Optional; [official release index](https://tauri.app/release/) |
| `vitest` | **5.0.4** | Registry version observed; [npm package](https://www.npmjs.com/package/vitest) |
| `@types/react` | **19.3.0** | Registry version observed; [npm package](https://www.npmjs.com/package/%40types/react) |
| `@types/node` | **24.19.2** | Registry reports this Node 24 line version; latest-in-line status **UNVERIFIED**; [npm versions](https://www.npmjs.com/package/%40types/node?activeTab=versions) |
| `@types/react-dom` | **UNVERIFIED** | Exact version not established; resolve a suitable React 19 type package before manifest creation |
| `pnpm` | **UNVERIFIED exact version** | Available per task context; record `pnpm --version` during implementation and pin it |

**UNVERIFIED integration gate:** the complete React/TypeScript/Vite/Vitest/RAC combination, peer ranges, Node 24 execution, and three-OS Cargo build have not been installed or tested. Confirm registry publication and peer compatibility, then record exact direct versions and resolved transitive dependencies in lockfiles. Do not turn this table into an untested claim of compatibility.

### CI actions

| Action | Verified major/reference | Exact immutable revision |
|---|---|---|
| `tauri-apps/tauri-action` | **v1**, current documented usage; [upstream](https://github.com/tauri-apps/tauri-action) | **UNVERIFIED** |
| `Swatinem/rust-cache` | **v2**, current documented usage; [upstream](https://github.com/Swatinem/rust-cache) | **UNVERIFIED** |

Resolve dated action revisions and the remaining explicitly UNVERIFIED pins when the phase 1 implementation begins. This report deliberately supplies no fabricated patch numbers, hashes, or passing-build claims.

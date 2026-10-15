# Import formats: existing Minecraft Java launcher instances

Research date: 2026-10-15. Scope: documentation for a from-scratch Rust/Tauri 2 launcher, Windows first, then macOS/Linux. Import means **copy into Nexora-owned storage**, leaving the source usable. Priority: official launcher, Prism/MultiMC, Modrinth, CurseForge, GDLauncher.

This document describes observed formats in original prose; it contains no copied launcher implementation. Reading GPL or other source here is format research, not permission to incorporate that source into the planned Apache-2.0 product. A source-tree observation is not a guarantee about every released build. **UNVERIFIED** marks a claim or compatibility candidate not established by the inspected sources. “Recommendation” identifies proposed Nexora behavior rather than an upstream guarantee.

Repository-tree snapshots observed during research: [Prism `60bcc269`](https://github.com/PrismLauncher/PrismLauncher/tree/60bcc2694d7433e2d2a657fbf62b0d022f9116e0), [Modrinth `5e3318bb`](https://github.com/modrinth/code/tree/5e3318bb4b8ec358b15f9fd9330f30534f90d27f), [GDLauncher Carbon `35533d7c`](https://github.com/gorilla-devs/GDLauncher-Carbon/tree/35533d7cd963be8955af4fe30f681a84ad527049), and [GDLauncher legacy `7e5346be`](https://github.com/gorilla-devs/GDLauncher/tree/7e5346be21bb67b2b3ebac7103ef3e76e02a558c). Individual branch-linked files can move; use these snapshots and installed-version fixtures when implementing adapters.

## Shared import contract

Recommendations:

- Separate launcher root, instance metadata directory, and game directory. The game directory is the working folder containing `mods/`, `config/`, `saves/`, etc.; it is not necessarily named `.minecraft`.
- Enumerate metadata first, present the resolved source directories and destination, then copy selected instances. Allocate new Nexora IDs; retain source IDs only as provenance.
- Preserve game content: mods, disabled mods, configs, worlds, resource/texture/shader packs, screenshots, scripts, datapacks, `options*.txt`, `servers.dat`, and other pack-specific content. Use an inventory plus explicit exclusions rather than a small whitelist that drops unknown mod data.
- Parse approved metadata fields into Nexora records. Do not copy whole launcher roots, global configuration, databases, authentication stores, browser profiles, or raw diagnostic dumps.
- Rebuild downloadable assets, libraries, natives and managed Java under Nexora. Preserve local patches/custom libraries separately when they are essential and cannot be reconstructed; do not silently label these imports vanilla.
- Keep unknown values unknown. Missing memory settings do not mean zero, missing play time does not mean never played, and a directory containing mod JARs does not prove a loader/version.
- Do not execute source launch hooks, installers or wrapper commands during discovery. Review JVM arguments for machine-specific paths and credentials before enabling them. Require Nexora sign-in independently.
- Copy files, not links to the source. Detect symlinks, junctions, external content-store links, cycles and path traversal; deliberately materialize selected content without walking arbitrary locations. Never create destination hard links to mutable source files.

## 1. Official Minecraft Launcher

### Locations and discovery

| Platform | Default Java data root |
| --- | --- |
| Windows | `%APPDATA%\.minecraft` |
| macOS | `~/Library/Application Support/minecraft` |
| Linux | `~/.minecraft` |
| Linux Flatpak package | `~/.var/app/com.mojang.Minecraft/.minecraft` |

These roots, including the Linux Flatpak fallback, are implemented by [Fabric Installer's directory discovery](https://github.com/FabricMC/fabric-installer/blob/master/src/main/java/net/fabricmc/installer/util/Utils.java). Probe all existing candidates; do not copy Fabric's fallback-only search policy.

Read both `launcher_profiles.json` and `launcher_profiles_microsoft_store.json` when present. [Fabric's profile writer](https://github.com/FabricMC/fabric-installer/blob/master/src/main/java/net/fabricmc/installer/client/ProfileInstaller.java) explicitly supports both names. Store/Xbox installation of the launcher executable does **not** imply that Java worlds moved into the Bedrock package directory. [Forge's Store-launcher announcement](https://forums.minecraftforge.net/announcement/16-known-issues-regarding-the-new-minecraft-launcher-from-the-microsoft-store/) documents the suffixed profile/account files in `.minecraft`; it is historical evidence, not a requirement that every current build use the suffix.

**UNVERIFIED:** `%LOCALAPPDATA%\Packages\Microsoft.4297127D64EC6_8wekyb3d8bbwe\LocalCache\Local\minecraft` as an alternative Java root for some Store installations. Do not treat the package path as authoritative without finding valid profile/version metadata. The executable package, launcher cache and Java game root are different concepts.

Recommendation: allow selecting a custom launcher root/profile JSON and external game directory. Custom profile `gameDir` values can point anywhere; there is no verified universal portable installation locator. **UNVERIFIED:** automatic root discovery from launcher shortcut arguments across all launcher generations. Deduplicate candidates by canonical location, but preserve distinct profile definitions pointing at the same game folder.

### Enumeration and fields

The top-level `profiles` object maps opaque profile keys to installation definitions; enumerate its entries, not only `versions/` directories. Preserve the key separately from the display name. Fields observed in the [MCProfiles reader's data model](https://github.com/sbehnke/MCProfiles/blob/main/profiles.go), an independent format reader rather than a Mojang schema, are:

| Value | Field / interpretation |
| --- | --- |
| Name | `name`; use a fallback label if absent |
| Selected version | `lastVersionId`; this may be a loader/custom version, not the base Minecraft version |
| Game directory | Nonempty `gameDir`; otherwise the Java data root |
| Java executable | `javaDir`; absence means launcher-managed/default selection |
| JVM/memory | `javaArgs`; extract `-Xms`/`-Xmx` with explicit unit conversion, retain other reviewed arguments |
| Icon | `icon`; named built-in icon or embedded image |
| Last use | `lastUsed`; a timestamp, not a duration |

Fabric's writer also establishes profile `type`, creation/use timestamps, and embedded icons. Its [icon helper](https://github.com/FabricMC/fabric-installer/blob/master/src/main/java/net/fabricmc/installer/util/Utils.java) emits a `data:image/png;base64,` URI. Decode with size/type limits; built-in icon names need a Nexora fallback. No cumulative official per-profile play-time field was verified; import duration as unknown. Do not substitute per-world player statistics for launcher play time.

### Minecraft and loader resolution

Look up `<root>/versions/<lastVersionId>/<lastVersionId>.json`. [Fabric's client installer](https://github.com/FabricMC/fabric-installer/blob/master/src/main/java/net/fabricmc/installer/client/ClientInstaller.java) creates `fabric-loader-<loaderVersion>-<gameVersion>` directories and corresponding JSON. Read JSON before parsing a directory-name heuristic: custom names are legal.

Recommendation for inference, using the selected version and its inherited parents:

1. Resolve `inheritsFrom` recursively with cycle/missing-parent checks; retain the complete chain. Use the underlying vanilla metadata to identify Minecraft. For moving selections such as `latest-release`/`latest-snapshot`, resolve against the official version manifest and record the resolved version; **UNVERIFIED:** exact moving-selection serialization in every official launcher release.
2. Identify Fabric from a `net.fabricmc:fabric-loader:<version>` library, Quilt from `org.quiltmc:quilt-loader:<version>`, Forge from `net.minecraftforge:forge:<mc>-<forge>`, and NeoForge from `net.neoforged:neoforge:<version>` when present. Treat these as evidence-based matching rules, not an exhaustive grammar. **UNVERIFIED:** coverage of the Quilt/NeoForge rules across all historical installer formats.
3. For Forge, corroborate with arguments such as `--fml.forgeVersion`/`--fml.mcVersion`, where present. The [observed CurseForge-generated Forge version JSON](https://github.com/EnigmaticaModpacks/CreateTogether/blob/fb56c63f0bbb5856cb865afd507017093fc857b2/minecraftinstance.json) contains `inheritsFrom`, Forge Maven coordinates and these arguments. Generic LaunchWrapper/ModLauncher main classes alone do not uniquely identify the loader.
4. Use `fabric-loader-*`, `*-forge-*`, `forge-*`, etc. only as fallback hints. If a custom version is unresolved, report “custom/unknown” and retain its metadata for a later compatibility adapter.

### Metadata, copy boundary and credentials

Profiles and version JSON are metadata. Selected game-directory content is the copy payload. `versions/`, `libraries/`, `assets/` and `runtime/` are shared installation material, not independent instances. Recommendation: reconstruct known vanilla/loaders; retain explicitly selected custom version JSON/JARs and required local dependencies when reconstruction is impossible. Shared game directories must be identified in the preview: two profiles can otherwise copy the same saves/mods twice.

**Do not import:** `launcher_accounts.json`, `launcher_accounts_microsoft_store.json` (both account filenames are evidenced by the Forge announcement), or legacy authentication objects inside profile JSON. **UNVERIFIED filename candidates, exclude if encountered:** `launcher_msa_credentials.json`, suffixed variants and `launcher_skins.json` as potentially account-associated launcher state. Never serialize unknown top-level profile objects such as `authenticationDatabase`, `clientToken`, selected-user data, or tokens into Nexora. Exclude launcher web caches/cookies and OS credential-store entries. Extract only installation fields from profile JSON; never copy the whole file.

## 2. Prism Launcher / MultiMC

### Locations and custom roots

| Platform/package | Prism root | MultiMC root |
| --- | --- | --- |
| Windows | `%APPDATA%\PrismLauncher` | Extracted launcher/executable directory |
| macOS | `~/Library/Application Support/PrismLauncher` | `<MultiMC.app>/Data`; older `<MultiMC.app>/Contents/MacOS` |
| Linux | `$XDG_DATA_HOME/PrismLauncher`, normally `~/.local/share/PrismLauncher` | Extracted executable directory; distro wrappers can override it |
| Prism portable | Launcher's own directory | MultiMC is also directory-oriented |
| Prism Flatpak | `~/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher` | **UNVERIFIED:** no universal MultiMC Flatpak root established |
| Prism Scoop | `%HOMEPATH%\scoop\persist\prismlauncher` | **UNVERIFIED:** package-dependent |

Prism's portable, ordinary, Flatpak and Scoop paths are documented in its [data-location guide](https://prismlauncher.org/wiki/getting-started/data-location/). The XDG variable form is a platform convention to validate for the installed build. MultiMC's executable-relative root, `--dir` override, macOS `Data` root and legacy macOS migration are observable in [MultiMC Application.cpp](https://github.com/MultiMC/Launcher/blob/develop/launcher/Application.cpp). Do not assume `%APPDATA%\MultiMC` is the universal Windows default.

Read `<root>/prismlauncher.cfg` or `multimc.cfg`, using INI/Qt-compatible values. `InstanceDir` defaults to `instances`; resolve relative paths against the effective launcher root, and honor absolute paths. Prism also has `AdditionalInstanceDirs` and `IconsDir`, shown in [its application settings registration](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/Application.cpp). Parse settings explicitly; global configuration may contain proxy credentials. [Prism's CLI documentation](https://prismlauncher.org/wiki/getting-started/command-line-interface/) confirms `-d`/`--dir` custom roots. For portable/custom installations, accept an explicit root or instances folder rather than scanning the entire disk.

### Enumeration, settings and components

Enumerate immediate instance directories containing `instance.cfg`, then parse `mmc-pack.json` where available. The [instance loader](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/InstanceList.cpp) is the format reference. Modern `InstanceType=OneSix` instances are the initial target; older legacy types need separate adapters, not guessed conversion.

`instance.cfg` generally stores values in `[General]`. Relevant fields from [BaseInstance.cpp](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/BaseInstance.cpp):

| Value | INI keys |
| --- | --- |
| Name/icon | `name`, `iconKey` |
| Play time | `totalTimePlayed` and `lastTimePlayed` in seconds; `lastLaunchTime` in epoch milliseconds |
| Pack provenance | `ManagedPack`, `ManagedPackType`, `ManagedPackID`, `ManagedPackVersionID` and descriptive companions |
| Commands/account association | `OverrideCommands`, pre/post/wrapper commands, and instance account association are not an instruction to execute/import credentials |

`iconKey` can identify a built-in resource or custom image in configured `IconsDir`; copy only a validated resolved image. Missing/built-in artwork can use a Nexora icon. Do not confuse launcher-wide `playtime.cfg` totals with per-instance totals.

Java/memory inheritance is established by [MinecraftInstance.cpp](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/MinecraftInstance.cpp): `OverrideJavaLocation` gates `JavaPath`; `OverrideJavaArgs` gates `JvmArgs`; `OverrideMemory` gates `MinMemAlloc`, `MaxMemAlloc`, `PermGen`. Memory allocations are in MiB-style megabyte JVM settings. When an override is off, read the global setting instead. `AutomaticJava` and newer global Java selection can affect the effective runtime; revalidate paths and runtime compatibility on the destination. Preserve the distinction between unspecified and explicitly empty JVM arguments.

`mmc-pack.json` is a versioned JSON component list: `formatVersion`, then `components[]` containing `uid`, `version` and optional descriptive/state fields. The [pack profile reader](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/PackProfile.cpp) also resolves local component patches under `patches/<uid>.json`.

| Component UID | Nexora interpretation |
| --- | --- |
| `net.minecraft` | Minecraft version |
| `net.fabricmc.fabric-loader` | Fabric loader version |
| `net.minecraftforge` | Forge version |
| `net.neoforged` | NeoForge version |
| `org.quiltmc.quilt-loader` | Quilt loader version |
| `org.lwjgl` / `org.lwjgl3` | LWJGL runtime component, not a mod loader |

The mod-loader mapping is explicit in [Component.cpp](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/Component.cpp); LWJGL identities appear in [MinecraftInstance.cpp](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/MinecraftInstance.cpp). Do not confuse intermediary/mapping components with loaders. Preserve unknown components as compatibility requirements.

### Game content, patches and mod sources

Current Prism selects `<instance>/minecraft` unless only `<instance>/.minecraft` exists; when both exist it chooses `minecraft`. This precedence is explicit in `gameRoot()` in MinecraftInstance.cpp. For MultiMC or ambiguous historical layouts, inspect both and report the selected directory; do not merge them automatically.

Metadata includes `instance.cfg`, `mmc-pack.json`, groups and icon references. Content is under the resolved game root. `patches/`, `jarmods/` and instance-local `libraries/` can contain essential custom launch material outside it. Preserve and inventory these separately; copying `mods/` alone will lose jar modifications. Recommendation: block a “ready to launch” result if required patches or jar-mod ordering cannot be represented. Skip reproducible natives, caches, logs and shared library/assets trees by default.

Prism writes packwiz-style TOML in `<game>/mods/.index/*.pw.toml` (and analogous resource directories). [Packwiz.cpp](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/modplatform/packwiz/Packwiz.cpp) establishes:

- Root `filename` maps metadata to the local payload; `name`/`side` are descriptive.
- `[download]` contains `hash-format`, `hash`, `mode`, and possibly `url`.
- `[update.modrinth]`: `mod-id` is the project ID, `version` the version ID.
- `[update.curseforge]`: numeric `project-id` and `file-id` identify the project/file.
- `x-prismlauncher-*` extensions carry loader/game compatibility, version display, dependencies and update-lock state.

Recover provider IDs before discarding/replacing metadata. Validate filename/hash correspondence; stale index entries are possible. MultiMC instances are not guaranteed to have this Prism metadata.

### Credential exclusions

Exclude root `accounts.json`, the `accounts/` directory if present, login caches, and all account/selected-account identifiers from translated instance settings. Prism's Application.cpp opens `accounts.json`; MultiMC's Application.cpp migration also lists `accounts` and `accounts.json`. Do not copy the entire global INI because proxy username/password and other machine settings coexist with useful defaults. **UNVERIFIED:** any exhaustive per-version list of additional auth-cache filenames; root-level copying is therefore disallowed by design.

## 3. Modrinth App

### Locations and split settings/content roots

| Platform | Default settings/app root |
| --- | --- |
| Windows | `%APPDATA%\ModrinthApp` |
| macOS | `~/Library/Application Support/ModrinthApp` |
| Linux | `$XDG_DATA_HOME/ModrinthApp`, normally `~/.local/share/ModrinthApp` |

The [official storage guide](https://support.modrinth.com/en/articles/8797641-modrinth-app-storage-folder-location) documents these paths, the configurable app directory, and the pre-0.8.0 identifier `com.modrinth.theseus`. Probe historical roots using that identifier too. **UNVERIFIED:** a universal Flatpak package ID/path; let the user select a sandbox/custom root rather than inventing one.

[dirs.rs](https://github.com/modrinth/code/blob/main/packages/app-lib/src/state/dirs.rs) distinguishes `settings_dir` from changeable `config_dir`. `THESEUS_CONFIG_DIR` can override the initial settings directory. Database `settings.custom_dir` selects the content root; `prev_custom_dir` can indicate relocation state. `profiles/` remains the on-disk instance folder name in the inspected source even after database terminology changes. [db.rs](https://github.com/modrinth/code/blob/main/packages/app-lib/src/state/db.rs) places `app.db` in the initial settings directory, uses SQLite WAL, and runs migrations. Therefore do not assume selecting a custom app directory moved `app.db` there.

### Legacy `profiles` schema

The [initial SQL migration](https://github.com/modrinth/code/blob/main/packages/app-lib/migrations/20240711194701_init.sql) defines the following relevant fields. Enumerate database rows, not arbitrary `profiles/` subfolders.

| Table | Relevant columns |
| --- | --- |
| `profiles` | `path` primary key; `name`, `icon_path`, `install_stage` |
| `profiles` version | `game_version`, `mod_loader`, nullable `mod_loader_version` |
| `profiles` launch | `override_java_path`, `override_extra_launch_args`, `override_mc_memory_max` |
| `profiles` provenance/time | `linked_project_id`, `linked_version_id`, `locked`, `created`, `modified`, `last_played`, `submitted_time_played`, `recent_time_played` |
| `settings` | `custom_dir`, `prev_custom_dir`, `extra_launch_args`, `mc_memory_max` |
| `java_versions` | `major_version`, `full_version`, `architecture`, `path` |

JSON-valued argument fields use SQLite JSONB, not necessarily UTF-8 JSON text. Normalize through SQLite JSON functions on a compatible SQLite version. Null override values inherit defaults; confirm array/empty override behavior against the installed schema. The loader strings are `vanilla`, `fabric`, `forge`, `quilt`, `neoforge` in [instance_types.rs](https://github.com/modrinth/code/blob/main/packages/app-lib/src/state/instance_types.rs). Keep loader version separate from Minecraft and preserve opaque version IDs rather than splitting them speculatively.

### New `instances` schema: drift is already observable

The [20260611 migration](https://github.com/modrinth/code/blob/main/packages/app-lib/migrations/20260611120000_instances-content-foundation.sql) adds these relationships:

| Table | Import-relevant fields |
| --- | --- |
| `instances` | `id`, `path`, `name`, `icon_path`, `applied_content_set_id`, timestamps and both play-time counters |
| `instance_content_sets` | `id`, `instance_id`, `game_version`, `loader`, `loader_version`, `status` |
| `instance_launch_overrides` | `instance_id`, `overrides` JSONB, including `java_path`, `extra_launch_args`, `memory.maximum` |
| `instance_links` | `instance_id`, `link_kind`, `modrinth_project_id`, `modrinth_version_id` and other provenance |
| `instance_files` | `id`, `instance_id`, `relative_path`, `enabled`, `sha1`, `size`, `missing` |
| `instance_content_entries` | `file_id`, `content_set_id`, `project_id`, `version_id`, `source_kind`, `enabled` |

Join the applied content set by ID **and instance ownership**, not every historical set. Associate file rows and content entries for mod-source recovery. The [20260619 migration](https://github.com/modrinth/code/blob/main/packages/app-lib/migrations/20260619120000_finish-instance-profile-migration.sql) drops `profiles`; hard-coding only that table will fail on newer databases. [Current row adapters](https://github.com/modrinth/code/blob/main/packages/app-lib/src/state/instances/adapters/sqlite/instance_rows.rs) corroborate these joins and show further optional icon configuration. Branch migrations demonstrate possible layouts, not the release installed on a user's machine.

Recommendation: inspect `sqlite_schema`, `PRAGMA table_info(...)` and migration records, choose an adapter by schema capabilities, and reject unknown required layouts without modifying them. Never run Modrinth migrations on the source. Use read-only, consistent SQLite reads with WAL awareness; `immutable=1` on a live changing database is inappropriate. Do not persist a raw DB snapshot in the imported instance.

### Content, launch settings, icons, time and credentials

Resolve each safe relative `path` under `<content-root>/profiles/`; the profile directory itself is the game directory, without an assumed `.minecraft` child. Copy content and materialize any shared-store references; dirs.rs also exposes a shared `store/content` tree, so assuming every payload is a regular independent file is unsafe. Extract an existing validated `icon_path` image; new generated-icon settings need a fallback when not representable.

Recommendation: obtain Java/memory/JVM overrides from the matching schema and merge with global defaults according to that version's semantics. Select a compatible Nexora runtime rather than copying Modrinth Java installations. Preserve `last_played` separately. **UNVERIFIED:** exact unit and reporting/acknowledgment semantics of `submitted_time_played` versus `recent_time_played` across releases; verify the installed version's accounting before summing them into a canonical duration.

**Credential-bearing tables in `app.db`:** `minecraft_users` (access/refresh tokens), `minecraft_device_tokens` (private key/device token), `modrinth_users` (session ID), as established by the initial migration. Exclude them, raw `app.db`, WAL/SHM companions, database backups, browser/webview storage and legacy auth stores. [minecraft_auth.rs](https://github.com/modrinth/code/blob/main/packages/app-lib/src/state/minecraft_auth.rs) confirms persisted access/refresh tokens. **UNVERIFIED:** historical standalone auth filenames; do not depend on a filename blacklist to make a whole-root copy safe.

## 4. CurseForge App

### Locations and discovery

| Platform | Default Minecraft modding root | Instance parent |
| --- | --- | --- |
| Windows | `%USERPROFILE%\curseforge\minecraft` | `<root>\Instances` |
| macOS | `~/Documents/curseforge/minecraft` | `<root>/Instances` |
| Linux | `~/Documents/curseforge/minecraft` | `<root>/Instances` |

The [official setup guide](https://support.curseforge.com/support/solutions/articles/9000218572) gives all three defaults and its Advanced custom-location option. [GDLauncher's CurseForge importer](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_app/src/managers/instance/importer/curseforge.rs) independently resolves home/Documents plus `curseforge/minecraft/Instances`.

Honor the user's configured Minecraft modding/install path; app installation and Minecraft modding folders differ. Recommendation: expose root/Instances/single-instance folder selection and validate `minecraftinstance.json`. **UNVERIFIED:** a stable cross-version on-disk setting filename/key for locating the custom path automatically, including differences between Overwolf and standalone builds. Likewise no verified universal portable/Flatpak root; do not invent one. Older Twitch/Overwolf paths should be user-selected discovery candidates until separately verified.

### Metadata and instance fields

Enumerate immediate directories containing `minecraftinstance.json`; that directory itself is the game directory. This is installed-app metadata, not the same schema as exported CurseForge `manifest.json`.

The upstream GDLauncher importer and [Modrinth's CurseForge reader](https://github.com/modrinth/code/blob/main/packages/app-lib/src/api/pack/import/curseforge.rs) verify `name`, `gameVersion`, optional `baseModLoader.name`, `profileImagePath` and `installedModpack.thumbnailUrl`. The GDLauncher reader additionally recognizes top-level `projectID`, `fileID`, `isUnlocked` and `lastPlayed`.

An [actual Create Together instance file at a fixed revision](https://github.com/EnigmaticaModpacks/CreateTogether/blob/fb56c63f0bbb5856cb865afd507017093fc857b2/minecraftinstance.json) establishes additional fields and an important spelling distinction:

| Value | Observed representation / import handling |
| --- | --- |
| Minecraft | Top-level `gameVersion`; corroborate `baseModLoader.minecraftVersion` and embedded JSON inheritance |
| Loader | `baseModLoader.name`, e.g. `forge-36.2.22`; `forgeVersion`, `mavenVersionString`, `versionJson` can corroborate it |
| Java/JVM | `javaDirOverride`, `javaArgsOverride`; null means no local override in this specimen |
| Memory | `isMemoryOverride`, `allocatedMemory`; only apply allocation as local override when enabled; **UNVERIFIED:** unit guarantee for every release |
| Icon | `profileImagePath`, else pack thumbnail; copy a local image or use a fallback |
| Play information | `lastPlayed`, `playedCount`; count is launches, not elapsed duration. Cumulative duration is **UNVERIFIED** |
| Pack IDs | Top-level `projectID`/`fileID` identify the modpack, not each mod |
| Installed mods | `installedAddons[]` in this specimen uses `addonID`, nested `installedFile.id`, `installedFile.fileName`, `installedFile.FileNameOnDisk`, `packageFingerprint` |

Do **not** assume `installedAddons[].projectID/fileID` is universal. Recommendation: accept those fields when verified for a supported schema, but support the observed `addonID` + `installedFile.id` shape and exported `manifest.files[].projectID/fileID` separately. **UNVERIFIED:** direct `projectID/fileID` on every installed-addon object. Match metadata to actual disk files and retain manually added JARs absent from `installedAddons`.

Modrinth's reader recognizes `forge-<version>` and `fabric-<loaderVersion>-<mcVersion>` names. GDLauncher's reader also handles Quilt-style suffix stripping. **UNVERIFIED:** exhaustive NeoForge/Quilt naming across CurseForge releases; use embedded version JSON/Maven coordinates and an explicit unknown-loader outcome instead of falling back silently to vanilla.

### Copy boundary and credentials

Copy the selected instance's game content, including custom scripts and local changes. Parse `minecraftinstance.json`, optional `manifest.json`, addon IDs and icon metadata into Nexora records rather than copying them as active Nexora configuration. The instance metadata can contain absolute `installPath` and sync/profile state; do not carry it wholesale. Do not copy shared `Install/`, launcher binaries, downloads, caches or natives as game content. Reconstruct known Minecraft/loaders.

**Do not import:** any official launcher `launcher_accounts*.json`, legacy auth-bearing `launcher_profiles*.json` objects, or credential-cache files encountered under the shared launcher/`Install` environment. Exclude CurseForge/Overwolf app profiles, cookies, local storage and account/session databases entirely. **UNVERIFIED:** exact CurseForge standalone/Overwolf credential filenames and OS keychain entries across versions. Accordingly, select game folders and metadata fields; never recursively import CurseForge's application-data root to “get settings.” Missing global Java defaults should remain unknown if their schema is not verified.

## 5. GDLauncher: legacy and Carbon are different formats

### Locations and generation detection

| Platform | Legacy app/data root | Carbon app root; default runtime is `<app-root>/data` |
| --- | --- | --- |
| Windows | `%APPDATA%\gdlauncher_next` | `%APPDATA%\gdlauncher_carbon` |
| macOS | `~/Library/Application Support/gdlauncher_next` | `~/Library/Application Support/gdlauncher_carbon` |
| Linux | `$XDG_CONFIG_HOME/gdlauncher_next`, normally `~/.config/gdlauncher_next` | `$XDG_DATA_HOME/gdlauncher_carbon`, normally `~/.local/share/gdlauncher_carbon` |

Legacy roots and custom handling are supported by [legacy electron.js](https://github.com/gorilla-devs/GDLauncher/blob/master/public/electron.js) and [Carbon's legacy importer](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_app/src/managers/instance/importer/legacy_gdlauncher.rs). Legacy `override.data` is a text path in the default root. Portable non-Linux legacy builds use `data` beside the executable. Honor an explicit folder too; no universal Linux portable locator was verified.

Carbon defaults and `gdl_conf.db` are documented in its [troubleshooting guide](https://gdlauncher.com/docs/troubleshooting/). [Carbon's Electron entrypoint](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/apps/desktop/packages/main/index.ts) resolves runtime from `--runtime_path`, then `GDL_RUNTIME_PATH`, then the app-root text file `runtime_path_override`, then `<app-root>/data`. **UNVERIFIED:** universal Flatpak-specific paths for either generation; use a selected sandbox/runtime root.

Detect legacy from `instances/<folder>/config.json` containing `loader` (or older `modloader`), with game content alongside it. Detect Carbon from runtime `gdl_conf.db` and `instances/<shortpath>/instance.json` with `_version`, plus an `instance/` child. Both generations may coexist. A file called `config.json` alone is not sufficient identification.

### Legacy fields and content

[getInstances.js](https://github.com/gorilla-devs/GDLauncher/blob/master/src/app/desktop/utils/getInstances.js) enumerates directories and derives display name from the directory name. It understands `loader.loaderType`, `loader.mcVersion`, `loader.loaderVersion`, optional pack `loader.projectID/fileID/source`, and earlier `modloader` arrays. It also translates historical `fileId`, `addonId`, `addonID` spellings. Nexora should normalize these in memory, never rewrite the source.

[Legacy launch actions](https://github.com/gorilla-devs/GDLauncher/blob/master/src/common/reducers/actions.js) use per-instance `customJavaPath`, `javaArgs`, `javaMemory`, falling back to global `settings.java` values. The [settings reducer](https://github.com/gorilla-devs/GDLauncher/blob/master/src/common/reducers/settings/reducers.js) defines global `path`, `pathLatest`, `memory`, `args`; the JVM builder in [desktop utils](https://github.com/gorilla-devs/GDLauncher/blob/master/src/app/desktop/utils/index.js) appends memory as `m` units.

The legacy importer verifies `background` as a relative image, `timePlayed` in minutes (convert to seconds), and `lastPlayed` in epoch milliseconds. Copy game content directly from the instance root, excluding parsed `config.json`, backup/temp configs, optional pack `manifest.json`, lock markers, generated natives and launcher imagery already imported separately. `mods[]` and manifests can recover project/file provenance; validate ID spelling and actual file association. Do not assume loader-level pack IDs identify individual mods.

### Carbon database, metadata and game folder

Carbon is **not database-only**. The [instance manager](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_app/src/managers/instance/mod.rs) reads/writes `instance.json`; [runtime path types](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_rt_path/src/lib.rs) put game files at `<runtime>/instances/<shortpath>/instance/`.

The [schema wrapper](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_app/src/managers/instance/schema/mod.rs) tags the JSON with `_version: "1"`. The [v1 schema](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_app/src/managers/instance/schema/v1.rs) provides:

| Value | JSON path |
| --- | --- |
| Name/icon/time | `name`, `icon` (null/default or relative image path), `last_played`, `seconds_played` |
| Minecraft | `game_configuration.version.release` for standard versions; custom version may instead be a string |
| Loader | `game_configuration.version.modloaders[]`: `type`, `version`; types include `Forge`, `Fabric`, `Quilt`, `Neoforge` |
| Memory | `game_configuration.memory.min_mb/max_mb` |
| JVM | `game_configuration.extra_java_args`, `global_java_args` |
| Java | `game_configuration.java_override`: externally tagged `Path` or `Profile` with optional value |
| Pack source | `modpack` with platform-tagged pack information, project/file or project/version IDs |

Read `gdl_conf.db` only for selected metadata/defaults. The [initial database migration](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_repos/prisma/migrations/20240120134904_init/migration.sql) establishes `Instance(id,name,shortpath,javaProfileId)`, `JavaProfile(javaId)`, `Java(path,major,...)`, and `AppConfiguration(javaCustomArgs,xmx,xms,...)`. Use `shortpath` to resolve the directory; do not derive it from the display name. Resolve JSON Java profile references through the DB where possible. Global/default merging and migration changes must be checked against the installed schema.

For mod IDs, the same migration links `ModFileCache(instanceId,filename,enabled,metadataId)` to `ModMetadata`, then `CurseForgeModCache(projectId,fileId)` or `ModrinthModCache(projectId,versionId)` by `metadataId`. Verify cached hashes against the copied file. Copy the `instance/` payload and a resolved icon; translate metadata. Exclude setup state, pidfiles, launcher logs, managed Java, shared assets/libraries and raw DB files. Unknown custom-version formats require an explicit compatibility result.

### Credential exclusions

Carbon's `gdl_conf.db` contains `Account.accessToken`, `msRefreshToken`, `idToken` and `AppConfiguration.secret` in the initial migration; [a later migration](https://github.com/gorilla-devs/GDLauncher-Carbon/blob/develop/crates/carbon_repos/prisma/migrations/20260120212303_add_gdl_token/migration.sql) adds `Account.gdlToken`. Never copy this database or WAL/SHM/backups into an imported instance. Select only allowed metadata columns; do not query/serialize account rows.

Legacy root `config.json` is global state, distinct from per-instance `config.json`; do not import it wholesale. [Persistence configuration](https://github.com/gorilla-devs/GDLauncher/blob/master/src/common/store/persistConfig.js) also persists `settings` and `app` in IndexedDB/localForage (`gdlauncher`, `gdlauncher_persist`). Exclude IndexedDB, Local Storage, cookies and app/account state. **UNVERIFIED:** the exhaustive legacy token filenames and exact token placement in every root-config generation. A safe importer reads only selected global Java defaults and instance metadata, never copies the Electron user-data profile.

## Comparison

| Priority/source | Enumeration authority | Game folder | Version/loader authority | Mod-source recovery | Main exclusion / uncertainty |
| --- | --- | --- | --- | --- | --- |
| 1. Official | Both profile JSON filename variants, `profiles` entries | `gameDir` or Java root | Selected version JSON and inheritance | Usually hashes; no verified per-mod catalog | Account JSON, auth objects; shared game dirs and custom versions |
| 2. Prism/MultiMC | `InstanceDir` directories with `instance.cfg` | `minecraft` or `.minecraft` | `mmc-pack.json` + local patches | Prism `.index/*.pw.toml`, pack provenance, hashes | `accounts.json`, global INI secrets; jar mods/unknown components |
| 3. Modrinth | `app.db`: `profiles` or newer `instances` | `<content-root>/profiles/<path>` | Legacy columns or applied content set | New file/content tables, hashes | Mixed credential DB; schema drift and shared content store |
| 4. CurseForge | `Instances/*/minecraftinstance.json` | Instance directory itself | `gameVersion`, `baseModLoader`, embedded version JSON | `installedAddons`, manifest IDs, fingerprint | App sessions/shared launcher auth; undocumented settings variants |
| 5a. GDLauncher legacy | `instances/*/config.json` | Instance directory itself | `loader` or legacy `modloader` | `mods[]`, manifests, hashes | Root config/Electron auth state; historical aliases |
| 5b. GDLauncher Carbon | DB `Instance` plus `instance.json` validation | `instances/<shortpath>/instance` | Versioned JSON + selected DB defaults | Mod cache joins, hashes/fingerprints | Credential-bearing `gdl_conf.db`; custom JSON/schema drift |

## Implementation checklist

These are recommendations for later implementation; this research task adds no product code.

- [ ] Implement adapters in the requested priority order, with independent platform path discovery and explicit root selection. Report launcher root, metadata path and resolved game directory separately.
- [ ] Collect consented, sanitized format fixtures before supporting a schema. Cover Windows, macOS, Linux, portable/custom roots, Prism Flatpak, legacy/current game-folder names, both official profile filenames, both Modrinth DB generations, and both GDLauncher generations. Never commit real tokens or raw user databases.
- [ ] Parse JSON/INI/TOML/SQLite with bounds and encoding handling. Preserve Qt INI semantics, optional/null/empty distinctions, unknown components, and argument quoting. Use capability-based DB schema detection and read-only transactions; never invoke upstream initialization/migration routines.
- [ ] Resolve base Minecraft and loader versions independently. Check inheritance cycles, missing parents, conflicting loaders, patch/jar-mod requirements and local libraries. Mark unsupported custom versions explicitly; never silently substitute vanilla/latest.
- [ ] Read effective Java/memory/JVM settings, normalize units, validate destination Java compatibility, and keep hooks/wrappers disabled unless separately reviewed. Treat arguments/environment values as potentially sensitive.
- [ ] Recover **existing mod-source metadata first**: Prism packwiz project/version IDs, CurseForge addon/file metadata, Modrinth content/file joins, GDLauncher legacy mods and Carbon cache joins. Keep modpack IDs separate from individual mod IDs. Check actual file names, enabled state and hashes; metadata can be stale.
- [ ] For unidentified files, compute SHA-512/SHA-1 and use Modrinth `GET /v2/version_file/{hash}?algorithm=sha512` or batched `POST /v2/version_files`. Retain returned project ID and version ID only after checking the returned file hash. The [official hash lookup documentation](https://docs.modrinth.com/api/operations/versionfromhash/) specifies whole-file hexadecimal hashes and supported algorithms; the [batch endpoint](https://docs.modrinth.com/api/operations/versionsfromhashes/) documents batching. A filename/mod-internal ID alone is not proof of a hosting project.
- [ ] For CurseForge, implement the platform's fingerprint algorithm and use `POST /v1/fingerprints/432` (Minecraft game ID) or the supported general fingerprint endpoint with Nexora's authorized API access. [CurseForge's API reference](https://docs.curseforge.com/rest-api/#get-fingerprints-matches) documents fingerprint requests, `x-api-key` and exact/fuzzy results. Verify the algorithm against [Prism's hash utility](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/modplatform/helpers/HashUtils.cpp): CurseForge fingerprints require MurmurHash2 and whitespace-byte normalization, not an arbitrary hash of the untouched JAR. Create independent fixtures; do not copy GPL implementation.
- [ ] Treat fingerprint collisions/fuzzy matches as candidates. Corroborate with size and available cryptographic hashes; preserve unknown/local mods and multiple provider associations instead of forcing a match. API failure or no match must not prevent local content copying.
- [ ] Enforce credential exclusions at every boundary: approved metadata fields/columns only; no account objects, raw app databases, Electron/WebView profiles, cookie stores or credential caches. Redact import diagnostics. Hash lookup sends hashes/IDs, not world files or credentials.
- [ ] Inventory content before copying, show shared-source duplication and disk requirements, handle links/store-backed files deliberately, stage under Nexora, verify copied file counts/sizes/hashes, and atomically publish a new instance. Cancellation/failure must leave the source untouched and no partially published instance.
- [ ] Preserve play time only where units/semantics are established (Prism seconds; GDLauncher legacy minutes; Carbon seconds). Keep timestamps separate and Modrinth/CurseForge unresolved duration semantics marked unknown.
- [ ] Finish each import with a report of copied content, reconstructed runtime dependencies, recovered mod origins, omitted credentials, unresolved custom features and missing files. Perform a launch check only as a later user action after independent Nexora authentication.

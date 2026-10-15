# Settings files and local instance synchronization

Research date: 2026-10-15. Scope: Minecraft **Java Edition**, Nexora Launcher, Rust/Tauri 2, same-PC storage, no cloud. This is documentation and a proposed design, not an implemented compatibility guarantee. No GPL implementation code is reproduced here.

## 1. Recommended scope and evidence standard

Use a shared, typed preference profile and project its selected fields into each instance immediately before launch. Read the instance again after the game exits, but record changes as proposals; do not automatically publish them back into the shared profile. This preserves the initial one-way contract while allowing a deliberate “Use these changes in the shared profile” action later.

Do not share a single physical `options.txt`, symlink instances to it, or copy whole files between game versions. Each instance needs its own version-specific file, defaults, mod-owned entries, and local state. Same PC reduces hardware differences but does not eliminate different monitors, audio devices, renderers, mod sets, or performance budgets.

Evidence labels used below:

- **Verified source** means an accessible upstream document, API reference, package publication, or maintainer-authored report supports the stated fact. It does not mean Nexora has run that game version.
- **UNVERIFIED** marks an unresolved historical boundary, serialization detail, or behavior needing a controlled client experiment. Such a rule must remain disabled until verified.
- **Recommendation** describes Nexora policy, not behavior promised by Minecraft.

The requested [Minecraft Wiki `options.txt` reference](https://minecraft.wiki/w/Options.txt) was blocked by the research browser. Its [accessible legacy-hosted copy](https://minecraft.fandom.com/wiki/Options.txt) supplies basic format terminology; it is not sufficient to certify every patch version. Primary references below support specific changes. No clients were launched and no fixtures were generated during this documentation task.

## 2. `options.txt`: grammar, encodings, and lifecycle

The file lives in the instance game directory. Its basic structure is one `key:value` record per line; `version` records the client's saved **DataVersion**, not its network protocol or resource-pack format. The format is not globally JSON or Java Properties. [Format reference](https://minecraft.fandom.com/wiki/Options.txt).

### Parsing and representation

Recommended parser behavior:

1. Split each record at its **first** colon. Preserve the remainder verbatim: values can contain further colons, including resource identifiers and server addresses.
2. Keep original bytes, line endings, ordering, empty values, duplicate occurrences, and unrecognized records in an instance-local document representation. Unknown entries are not eligible shared preferences.
3. Decode only allowlisted keys using the adapter for the target version. Reject ambiguous duplicates for a managed key rather than relying on undocumented precedence. A generic Properties parser would introduce escaping, comments, continuation, and delimiter rules that this file does not promise.
4. Keep raw text distinct from semantic values. A boolean, numeric scalar, enum integer, JSON array, bare string, and JSON string require different writers. Never put quotation marks around every string or apply JSON decoding to every value.
5. Reject non-finite floats and malformed selected values; leave the original file recoverable. Preserve unsupported values without exporting them to other instances. Do not interpret absence as the user's request to reset all targets.

Illustrative encodings, not a complete schema:

| Encoding | Example record | Nexora handling |
| --- | --- | --- |
| Boolean | `autoJump:true` | Boolean codec, lowercase output. |
| Decimal scalar | `mouseSensitivity:0.5` | Store semantic number and original lexical form separately. |
| Integer/enum | `chatVisibility:0` | Validate the enum for the target, not merely that it is an integer. |
| JSON string array | `resourcePacks:["vanilla","file/Example.zip"]` | Parse the complete array, preserving order; validate pack availability separately. |
| Bare language identifier | `lang:en_us` | Language registry identifier, not a translated display name. |
| Key translation identifier | `key_key.forward:key.keyboard.w` | Device/type-aware key codec. |
| Quoted JSON string | A selected field may contain `"..."`, including escaped characters | Use only when the exact target field's codec requires JSON strings. **UNVERIFIED:** a universal per-version list of quoted-string fields was not established. In particular, do not assume every version writes `soundDevice` with the same quoting. |

The array example is illustrative; it does not establish that every release writes the built-in pack into the list. **UNVERIFIED:** exact UTF-8/default-charset behavior across all historical releases, duplicate-key precedence, and malformed-line recovery. Preserve source encoding until an adapter has a verified fixture; use UTF-8 for verified modern targets.

### Migration and save behavior

Modern mappings expose distinct `load`, NBT `update`, option visitation, and `write` operations. Minecraft also has specific data-fixer classes for old key codes, key translation names, language casing, and other option migrations. This establishes version-aware conversion machinery, not a public stable launcher API. [GameOptions API, 1.21.8](https://maven.fabricmc.net/docs/yarn-1.21.8%2Bbuild.1/net/minecraft/client/option/GameOptions.html), [Fabric mappings index of options fixes](https://maven.fabricmc.net/docs/yarn-22w43a%2Bbuild.3/index-files/index-15.html).

Unknown keys are **not dependable persistent storage**. A mod author's description of [Remember My Txt](https://modrinth.com/mod/remember-my-txt/) documents vanilla losing mod keybinding entries when the owning mod is absent: unread entries are not emitted by the normal save path. Thus, a line surviving a read does not mean it survives a later save. Keep Nexora metadata outside game-owned files.

Practical answers for the launcher:

| Question | Working conclusion and limit |
| --- | --- |
| What happens to missing recognized options? | Leave them to the target's defaults and migration logic. **UNVERIFIED:** exact defaults and upgrade-specific default changes for every release; extract fixtures rather than hard-code one global default set. |
| Does the game keep unknown keys? | Do not rely on it. Known behavior includes loss on save when no option/mod consumes the entry; modded preservation patches may change this. |
| Does it rewrite the file? | The normal save path serializes current options rather than acting as a lossless text editor. Settings changes can save during the session; exit is not the only relevant boundary. **UNVERIFIED:** exact automatic save triggers and startup-only rewrites for each release. |
| Does loading always rewrite immediately? | **UNVERIFIED; do not assume so.** Separate load-time transformation from a later disk write. |
| Can a newer file be used by an older client? | There is no general downgrade contract. Project supported preferences into the older target's own file instead. |
| Can Nexora set `version` to force conversion? | No. It is provenance for the whole document. Changing it without converting the complete document can select inappropriate fixes. |

Recommendation: when patching an existing target file, retain its genuine `version` and local state; encode selected values for that document/target compatibility path. If the file was last written by another client version, resolve that migration first or skip application. For a new instance with no verified default fixture, allow the game to initialize the file and apply preferences on the next launch, or use a separately validated bootstrap adapter. Never mix modern key names into a legacy document and then falsely mark it as already migrated.

## 3. Version changes and an initial option registry

### Compatibility boundaries

| Release boundary | Relevant change | Consequence for sync |
| --- | --- | --- |
| 1.11 / 1.12 language era | Language resource identifiers became lowercase; modern options use forms such as `en_us`, while older installations may contain `en_US`. Lowercase resources are documented for 1.11+, and a dedicated language-options fixer exists. **UNVERIFIED:** exact first snapshot and every client-side `lang` transition detail. [Forge resource documentation](https://docs.minecraftforge.net/en/1.12.x/concepts/resources/), [options-fixer index](https://maven.fabricmc.net/docs/yarn-22w43a%2Bbuild.3/index-files/index-15.html). | Resolve the language against the destination's registry. Do not uppercase/lowercase arbitrary strings, pack filenames, or mod IDs as a general migration. |
| Through 1.12.2 → 1.13 | LWJGL2 integer key bindings give way to the LWJGL3/GLFW-era translation identifiers, commonly `key.keyboard.*` and `key.mouse.*`. The porting author describes the backend transition; later Forge documentation distinguishes key symbols, scancodes, and mouse buttons. [1.13 porting notes](https://gist.github.com/williewillus/353c872bcf1a6ace9921189f6100d09a), [Forge key mappings](https://docs.minecraftforge.net/en/1.20.1/misc/keymappings/). | Convert through a verified lookup table. Legacy integers are not GLFW integers. Preserve mouse and unbound distinctions. |
| Resource-pack discovery changes | Modern list entries can use repository IDs such as `file/Example.zip`, rather than a bare filename. **UNVERIFIED:** exact release/snapshot boundary and legacy built-in pack representations. | Use separate adapters for bare filenames, `file/` IDs, and built-in/mod pack IDs. Do not blindly prepend `file/` to every entry. |
| 1.16 | Fabulous graphics arrived; historical `fancyGraphics` boolean becomes the `graphicsMode` era. **UNVERIFIED:** exact snapshot for the serialized rename. [Official 1.16 prerelease notes](https://www.minecraft.net/en-us/article/minecraft-1-16-pre-release-3). | Boolean-to-enum conversion is lossy in reverse: Fabulous has no boolean equivalent. Make fallback explicit. |
| 1.19 | Darkness accessibility control and signed-chat-related preferences arrive. [Official 1.19 notes](https://feedback.minecraft.net/hc/en-us/articles/6731464524941-Minecraft-Java-Edition-1-19). | Only emit controls the target supports. A shared privacy preference must not claim to change account or server enforcement. |
| 1.19.1 → 1.19.3 | Chat-preview behavior changes in 1.19.1; preview is removed in 1.19.3. Extra telemetry becomes an opt-in setting in 1.19.3. [1.19.1 notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-19-1), [1.19.3 notes](https://feedback.minecraft.net/hc/en-us/articles/11280166737293-Minecraft-Java-Edition-1-19-3). | Do not retain obsolete preview controls as active preferences; exclude telemetry/consent state by default. |
| 1.19.4 | Damage tilt, glint speed/strength, and other accessibility controls expand. Auto-jump's default becomes off. [Official notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-19-4). | Missing is not equivalent to a stable historical default. Keep these preferences independent of graphics presets. |
| 1.21.6 | UI sounds gain their own volume category; music frequency and music-toast controls are added. [Official notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-6). | Earlier versions have no independent UI-volume destination. Do not redirect UI volume into master volume. |
| 1.21.9 | Horizontal mouse inversion is added. [Official notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-9). | Treat X and Y inversion independently; do not implement X inversion by changing the older Y setting. |
| 1.21.11 | Graphics modes become presets with separately configurable leaves, vignette, weather radius, and improved transparency. Debug keys and additional display/effect keys become rebindable. [Official notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-1-21-11). | End the blanket `graphicsMode` adapter here. A UI preset label is not proof of a stable serialized field or equivalent set of effects. New keybinding IDs require a new action registry. |
| 26.3 | SDL3 replaces GLFW; physical keyboard scancodes now drive gameplay bindings. [Official release notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-3), [upstream snapshot detail](https://feedback.minecraft.net/hc/en-us/articles/47424728811149-Minecraft-Java-Edition-26-3-Snapshot-4). | “1.13+” is no longer a sufficient codec family. **UNVERIFIED:** complete 26.3 disk binding grammar and migration behavior. Disable cross-boundary key sync until fixtures confirm it. |

### Practical option table

This is a starter allowlist design, not an exhaustive list of all options. Version intervals below are release-family scopes. **UNVERIFIED:** complete patch-by-patch validity and numeric validation of every row; no row alone authorizes a serializer. “Fixture-gated” means confirm the named disk key, codec, bounds, and round trip in each supported adapter. End dates are intentionally conservative where later versions changed the subsystem.

Core format identifiers are described by the [options reference](https://minecraft.fandom.com/wiki/Options.txt); modern field presence/types can be cross-checked against the [1.21.8 GameOptions API](https://maven.fabricmc.net/docs/yarn-1.21.8%2Bbuild.1/net/minecraft/client/option/GameOptions.html). Notes in the final column are Nexora recommendations.

| Option key / family | Intended valid version scope | Value type / allowed shared range | Category | Safe-to-sync notes |
| --- | --- | --- | --- | --- |
| `key_key.*` | Through 1.12.2 | LWJGL2 integer; unbound and mouse sentinels need lookup | keys | Store semantic action and device; never reinterpret raw integer in a later backend. **UNVERIFIED:** full legacy sentinel/key map. |
| `key_key.*`, registered mod action entries | 1.13–26.2 | Translation ID; keyboard, mouse, or scancode variant | keys | Match the exact registered action ID. Unknown mod actions stay local. Chords/modifiers need loader-aware adapters. |
| Rebindable debug actions | 1.21.11+ | Binding; exact added disk IDs **UNVERIFIED** | keys | Keep disabled until the destination action registry is known; do not invent IDs from translated labels. |
| `mouseSensitivity` | 1.12–1.21.x baseline; later fixture-gated | Float, shared policy 0–1 | mouse | Store normalized slider position; not degrees/count or operating-system pointer speed. |
| `invertYMouse`, `rawMouseInput`, `mouseWheelSensitivity`, `discrete_mouse_scroll` | Modern fixture-gated; not all exist in 1.12 | Boolean except positive wheel scalar; exact wheel limits **UNVERIFIED** | mouse | Separate hardware sampling from gameplay sensitivity; preserve per-instance overrides. |
| Horizontal inversion field | 1.21.9+ | Boolean; exact serialized key **UNVERIFIED** | mouse | Add only after disk-name verification. |
| `soundCategory_master`, `_music`, `_record`, `_weather`, `_block`, `_hostile`, `_neutral`, `_player`, `_ambient`, `_voice` | 1.12–1.21.x baseline; fixture-gated later | Float, shared policy 0–1 | audio | Names are singular where shown; do not substitute UI labels such as “blocks” or “records.” |
| `soundCategory_ui` | 1.21.6+; disk spelling fixture-gated | Float, shared policy 0–1 | audio | UI category availability is verified; older versions skip this preference. |
| `soundDevice` | Modern fixture-gated | Device string, possibly codec-quoted | audio | Exclude from ordinary volume sync; unavailable devices and quoting require local resolution. |
| `fancyGraphics` | 1.12–1.15.2 baseline | Boolean | graphics | Convert into a semantic Fast/Fancy preference, not a preserved alias alongside a newer field. |
| `graphicsMode` | 1.16–1.21.10 baseline | Enum integer, historically 0/1/2 | graphics | Fixture-gated enum mapping; Fabulous downgrade needs explicit fallback. Do not extend unchanged through 1.21.11. |
| `renderDistance`, `simulationDistance` | Render: baseline 1.12+; simulation: 1.18+ | Integer chunks; target-specific bounds **UNVERIFIED** | graphics | Opt-in performance group; clamp only with an explicit supported rule and show any adjustment. |
| `maxFps`, `enableVsync`, `mipmapLevels`, `particles` | Baseline 1.12+; later fixture-gated | Integer/boolean/enum; bounds vary | graphics | Different mod renderers can supersede these fields; define one owner per semantic preference. |
| `gamma` | Baseline 1.12+ | Float, shared policy 0–1 | graphics/accessibility | Do not export out-of-range fullbright hacks as portable preferences. **UNVERIFIED:** exact 1.19 clamp/reset path. |
| `fov` | Baseline 1.12+ | Normalized scalar; disk/UI conversion fixture-gated | graphics/accessibility | Never write the UI degree label directly into a normalized disk value. **UNVERIFIED:** complete historical mapping and bounds. |
| `bobView`, `fovEffectScale`, `screenEffectScale` | Bobbing baseline; effects modern fixture-gated | Boolean / floats 0–1 policy | accessibility | Preserve independently of presets. Exact effect-field introduction boundaries **UNVERIFIED**. |
| `chatVisibility`, `chatColors`, `chatLinks`, `chatLinksPrompt` | Baseline 1.12+ | Visibility enum; booleans | chat | Sync user preference only; do not weaken link prompts by default. |
| `chatOpacity`, `chatScale`, `chatWidth`, `chatHeightFocused`, `chatHeightUnfocused` | Baseline 1.12+ | Floats, shared policy 0–1 | chat | Share normalized values, not pixel sizes. Screen dimensions can alter the result. |
| `textBackgroundOpacity`, `backgroundForChatOnly`, `chatLineSpacing`, `chatDelay` | Modern fixture-gated | Float / boolean; delay nonnegative, maximum **UNVERIFIED** | chat/accessibility | Keep independent preferences; background rules are not necessarily equivalent to old chat-opacity semantics. |
| `darknessEffectScale` | 1.19+ | Float, shared policy 0–1 | accessibility | Does not remove all darkness/gameplay effects. |
| `damageTiltStrength`, `glintSpeed`, `glintStrength` | 1.19.4+ | Floats, shared policy 0–1 | accessibility | Validate exact target codec. Avoid silently replacing accessibility choices when graphics change. |
| `showSubtitles`, `narrator`, `toggleCrouch`, `toggleSprint` | Modern fixture-gated; narrator 1.12+ | Booleans except narrator enum | accessibility | Narrator modes and availability require target validation; per-user preference, not pack state. |
| `autoJump`, `autoSuggestions`, `lang` | Auto-jump baseline; suggestions 1.13+; language target-gated | Boolean / boolean / registry string | misc | Sync only explicit selections; validate installed language and preserve changed defaults. |
| `resourcePacks`, `incompatibleResourcePacks` | Multiple historical codecs; target-gated | Ordered JSON string arrays | misc | Off by default. Never transport compatibility acknowledgments automatically. |
| `fullscreen`, `fullscreenResolution`, `overrideWidth`, `overrideHeight` | Version-dependent local display fields | Boolean / mode string / integers | graphics | Exclude by default; resolution and size overrides should stay instance-local. |
| `lastServer`, `tutorialStep`, `joinedFirstServer`, `skipMultiplayerWarning` | Version-dependent local state | String / enum / booleans | misc | Never part of normal preference sync. |

The actual sound category enum includes identifiers such as records, blocks, players, and UI at the API level; distinguish enum constants from serialized names. [SoundCategory API](https://maven.fabricmc.net/docs/yarn-1.21.8%2Bbuild.1/net/minecraft/sound/SoundCategory.html). **UNVERIFIED:** exact removal/introduction boundary for every legacy sound key. Do not infer disk names from enum constant spelling.

Mouse sensitivity needs particular care. The familiar UI scale is a percentage-like presentation of an internal slider value, not a portable physical sensitivity measurement. **UNVERIFIED:** the exact percentage display formula and nonlinear camera transform across all backends. V1 should preserve a verified normalized slider value within a codec family and make no cm/360 equivalence claim. Raw input, OS acceleration, DPI, and the 26.3 physical-key change are separate concerns.

## 4. Categories and state to exclude by version

Use an allowlist plus explicit exclusions, with the exclusions taking priority. The entries below are policy even when an exact historical introduction is **UNVERIFIED**; exclude a named field whenever present rather than writing it into versions that lack it.

| Version family | Exclude from shared profile | Reason / exception |
| --- | --- | --- |
| All supported versions | Unknown keys, `version`, migration bookkeeping, diagnostics, debug verbosity, temporary capture state | Preserve locally; provenance is not a preference. Unknown entries can belong to absent mods. |
| All supported versions | `lastServer`, connection history and direct-connect values | Optional server-list sync is a different feature. Avoid changing the next connection target implicitly. |
| All supported versions | `overrideWidth`, `overrideHeight`, fullscreen mode/resolution and monitor selection | Display/window policy belongs to an instance. Same PC does not guarantee the same active monitor configuration. |
| All pack-capable versions | Pack selections referencing missing files; `incompatibleResourcePacks`; mod/built-in pack IDs unavailable in the target | Do not copy files or acknowledge incompatibility as a side effect. A separate opt-in adapter may sync a validated ordered selection. |
| 1.12 tutorial era onward, where present | `tutorialStep`, `joinedFirstServer`, onboarding counters, tutorial and toast completion state | These are progression or acknowledgment state, not ordinary gameplay preferences. |
| Multiplayer-warning versions, where present | `skipMultiplayerWarning`, chat-warning acknowledgments, ban notices and other consent flags | Do not suppress information in another instance. Exact field inventory is **UNVERIFIED** and must be expanded from fixtures. |
| Telemetry-bearing versions, especially 1.19.3+ | `telemetryOptInExtra` and related consent/telemetry state | Keep outside general settings sync; a graphics/audio action must not change consent. |
| Modern audio versions | `soundDevice`, device-dependent controls | Keep local unless explicitly opted into a device-resolving adapter. Volume sliders can still sync. |
| 1.19–1.19.2 chat-preview era | Preview settings when projecting to 1.19.3+ | Removed semantics have no useful destination. Preserve raw legacy values only in their own instance. |
| 1.21.11+ | Blind replay of a legacy `graphicsMode` or invented preset field | Independent effects need their own rules. An accessible UI label does not identify a disk codec. |
| 26.3+ | Raw GLFW/scancode strings imported from 1.13–26.2 without conversion | Input backend semantics changed. Unknown bindings should produce a skipped-field report, not a guessed key. |
| Any modded instance | Server/world config, registry/content toggles, hardware workarounds, caches, fingerprints, generated files, paths, credentials, machine IDs | A file's presence in `config/` does not make it a portable client preference. |

Pack opt-in should require matching destination pack identity, supported pack format, and existence, ideally with content hash evidence. Preserve ordering; skip the entire selection if dependencies or required packs cannot be resolved. Do not treat a filename match as proof that two packs contain the same content. The 1.21.11 graphics restructure and 26.3 backend change are documented in the official notes linked above; the exclusions are Nexora's conservative response to those boundaries.

## 5. DataVersion mapping

These are exact release IDs, not inclusive ranges and not guessed values for neighboring patches. The maintainer-published [PrismarineJS version dataset](https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/common/protocolVersions.json) explicitly separates `dataVersion` from protocol `version`. The latest stable release found in that dataset and [Mojang's release notes](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-3) is **26.3**. Later snapshots exist; they do not extend stable support automatically.

| Java release | DataVersion |
| --- | ---: |
| 1.12 | 1139 |
| 1.12.1 | 1241 |
| 1.12.2 | 1343 |
| 1.13 | 1519 |
| 1.13.1 | 1628 |
| 1.14 | 1952 |
| 1.14.4 | 1976 |
| 1.15 | 2225 |
| 1.15.1 | 2227 |
| 1.15.2 | 2230 |
| 1.16 | 2566 |
| 1.16.1 | 2567 |
| 1.17 | 2724 |
| 1.18 | 2860 |
| 1.19 | 3105 |
| 1.19.3 | 3218 |
| 1.19.4 | 3337 |
| 1.20 | 3463 |
| 1.20.1 | 3465 |
| 1.20.3 | 3698 |
| 1.20.4 | 3700 |
| 1.21 | 3953 |
| 1.21.1 | 3955 |
| 1.21.2 | 4080 |
| 1.21.3 | 4082 |
| 1.21.4 | 4189 |
| 1.21.5 | 4325 |
| 1.21.6 | 4435 |
| 1.21.7 | 4438 |
| 1.21.8 | 4440 |
| 1.21.9 | 4554 |
| 1.21.10 | 4556 |
| 1.21.11 | 4671 |
| 26.1 | 4786 |
| 26.1.1 | 4788 |
| 26.1.2 | 4790 |
| 26.2 | 4903 |
| 26.3 | 5023 |

This covers each major release from 1.12 and selected patches relevant to compatibility boundaries; it is deliberately not a complete patch/snapshot catalog. Unlisted version mappings are **UNVERIFIED in this document**. Resolve them from verified release metadata rather than interpolating. Before enabling an adapter, cross-check the chosen client artifact's embedded `version.json`/world-version metadata where available; **UNVERIFIED:** exact embedded metadata availability in every old client.

Track release ID, DataVersion, loader, and mods separately. DataVersion is a useful migration selector but does not describe a mod config schema. A numeric ordering alone does not establish downgrade safety or distinguish unusual version branches. The target game version is known from the launch manifest; the options file's `version` describes who last saved that document. A mismatch needs handling, not overwriting.

## 6. Optional `servers.dat` synchronization

The Java server list is **uncompressed, big-endian NBT** in the game directory, with a root compound containing a `servers` list of compounds. It is not gzip-compressed `level.dat`, JSON, or the newer nameless network-NBT envelope. The format does not represent LAN discovery or the direct-connect address. [Minecraft Wiki format reference](https://minecraft.fandom.com/wiki/Servers.dat_format), [NBT format and server-list example](https://c4k3.github.io/wiki.vg/NBT.html).

| Field | NBT type / meaning | Recommended sync policy |
| --- | --- | --- |
| Root `servers` | List of compounds | Preserve visible ordering; validate list type and bounded entry count. |
| `name` | String, user-facing server name | Include only with explicit server-list opt-in. Preserve Unicode and formatting. |
| `ip` | String, address including optional port | Preserve original text, including IPv6 notation; do not naïvely split on colons. |
| `icon` | Optional string, Base64 PNG cache | Exclude from shared profile; retain local cache for unchanged entries if practical. |
| `acceptTextures` | Optional byte, resource-pack choice | Preserve tri-state semantics: absent means prompt; 0 rejects; 1 accepts. Default shared policy should leave acceptance unset. |
| `hideAddress` | Historical tag, removed in older releases | Do not create for modern targets; exact historical transition is not needed for v1. |
| Additional fields | Version/mod-specific NBT | Preserve destination tags; do not project them without a schema. **UNVERIFIED:** complete current 26.3 field set, including secure-chat acceptance or hidden-server storage. |

Use NBT tag-preserving edits rather than deserializing into a struct that discards everything unknown. Bound nesting, file size, strings, list sizes, and icon decoding. Java NBT strings need modified UTF-8 handling; avoid a generic UTF-8-only binary parser. The NBT reference distinguishes byte order, tag types, and compression choices; confirm supplementary Unicode round trips in the chosen Rust library.

Recommendation: implement server-list sync as a separately toggled module with an explicit ordered replace/merge policy. A shared server entry should have a launcher UUID; game records do not provide that identity. Address/name matching is only a reconciliation heuristic and can be ambiguous. Do not propagate deletion just because a parse failed or a record disappeared after a migration. Keep resource-pack permission decisions local unless separately requested.

### Rust NBT crate candidates

Latest published versions visible in the documentation on the research date:

| Crate | Version | Declared license | Fit and caveats |
| --- | --- | --- | --- |
| `fastnbt` | **2.6.3** | **MIT OR Apache-2.0** | Serde integration and a tag-preserving `Value` model make it a reasonable default candidate for this small file. Dedicated NBT array types matter for general round trips. [Published version/docs](https://docs.rs/crate/fastnbt/2.6.3), [upstream license declaration](https://raw.githubusercontent.com/owengage/fastnbt/master/fastnbt/Cargo.toml). |
| `simdnbt` | **0.10.0** | **MIT** | Borrowed/owned APIs; use the ordinary named-root reader. Upstream calls Serde support experimental and documents deferred string validation in borrowed parsing. A tiny list does not justify choosing it solely for benchmark throughput. [Published docs](https://docs.rs/crate/simdnbt/0.10.0), [upstream manifest/license](https://github.com/azalea-rs/simdnbt/blob/master/simdnbt/Cargo.toml). |
| `quartz_nbt` | **0.2.9** | **MIT** | General NBT tree and explicit `Flavor::Uncompressed`; compression and SNBT support are also available. [Published docs](https://docs.rs/crate/quartz_nbt/0.2.9), [upstream license](https://github.com/Rusty-Quartz/quartz_nbt). |

License evidence above is the upstream project declaration, not a full dependency audit. The fetched `fastnbt` branch manifest still identified 2.6.2 while the package documentation identified 2.6.3; the published package is the evidence for the current version. Recheck the selected release archive's license and dependency notices before dependency adoption. These declarations support an Apache-2.0-planned launcher dependency evaluation without importing GPL launcher code. No crate was installed or evaluated by a build in this task.

## 7. Other settings and mod compatibility

### File inventory

All paths below are relative to the instance game directory unless stated otherwise.

| File / family | Format and verified scope | Recommended handling |
| --- | --- | --- |
| `optionsof.txt` | OptiFine settings sidecar, commonly colon-separated. **UNVERIFIED:** complete grammar, encoding, and version-specific schemas; the upstream [OptiFine documentation repository](https://github.com/sp614x/optifine) does not establish a universal schema here. | Off by default; build-specific adapter. Do not assume vanilla option names or ranges. |
| `optionsshaders.txt` | OptiFine shader preferences sidecar, commonly Properties-style. **UNVERIFIED:** exact top-level codec/encoding and field inventory for each OptiFine build. Upstream shader documentation describes shader options, not a stable launcher persistence API. [Shader documentation](https://github.com/sp614x/optifine/blob/master/OptiFineDoc/doc/shaders.txt). | Match OptiFine build, shaderpack identity, and pack option schema. Never infer format merely from `.txt`. |
| `config/iris.properties` | The Iris 26.1 branch resolves this filename under its config directory. The config implementation uses Java Properties streams with ISO-8859-1 plus Unicode escapes. Fields include shader selection, enablement, shadow distance, and other local controls. [Path owner](https://github.com/IrisShaders/Iris/blob/26.1/common/src/main/java/net/irisshaders/iris/Iris.java), [config implementation](https://raw.githubusercontent.com/IrisShaders/Iris/26.1/common/src/main/java/net/irisshaders/iris/config/IrisConfig.java). | `Iris config/iris.properties` means the usual config-directory file, not a required literal `Iris/` directory. Enablement/selection only after checking the selected pack exists; exclude debug/update/consent state. Do not copy implementation code. |
| Shaderpack option sidecars | Iris reads separate pack option properties; option identity depends on the chosen shaderpack. [Iris owner implementation](https://github.com/IrisShaders/Iris/blob/26.1/common/src/main/java/net/irisshaders/iris/Iris.java). **UNVERIFIED:** universal naming/location across Iris and OptiFine generations. | Same shader filename is insufficient. Require content identity or an explicit supported pack-version range and schema. |
| `config/sodium-options.json` | Sodium's upstream documentation identifies this as user-configurable settings and says deletion regenerates defaults. [Sodium configuration guide](https://github.com/CaffeineMC/sodium/wiki/Configuration-File). | Select known client-facing fields for the exact Sodium schema. Exclude driver workarounds and machine tuning unless specifically opted in. |
| `config/sodium-mixins.properties`, `config/sodium-fingerprint.json` | Upstream describes debugging/compatibility overrides and installation fingerprinting respectively. [Sodium guide](https://github.com/CaffeineMC/sodium/wiki/Configuration-File). | Never include in normal settings sync, even on the same PC. |
| Controlling and vanilla-registered mod keys | Controlling adds search/control to the keybinding UI; it does not establish a universal separate binding database. [Controlling upstream](https://github.com/jaredlll08/Controlling). Loader APIs register actions with Minecraft's keymapping system. [Forge documentation](https://docs.minecraftforge.net/en/1.20.1/misc/keymappings/). | Discover action ownership. Do not invent `controlling-keybinds.json`. **UNVERIFIED:** exact persistence hooks of every Controlling release and other keybinding mods. |
| Chord, alternate-binding, macro, and profile mods | May extend normal bindings or own extra files. **UNVERIFIED:** generic format; none exists in the consulted loader APIs. | Adapter per owner/schema. Preserve modifier/context distinctions and prohibit a lossy chord-to-single-key conversion by default. |
| Other `config/` files | Mods can choose JSON, TOML, JSON5, Properties, or custom formats. NeoForge documents client/common/server/startup roles and per-world server overrides. [NeoForge configuration documentation](https://docs.neoforged.net/docs/misc/config/). | Parse by declared format, not extension alone; inspect ownership and purpose. Exclude world/server/content config from v1. |

JSON5 comments/trailing commas, TOML dotted tables and types, and Java Properties escaping/continuations require different parsers. Prefer lossless document editing when comments matter; preserve unrelated fields. A mod-specific writer may discard unknown properties even if Nexora preserves them initially. Do not promise that every mod will preserve formatting after its next save.

### Deciding “same mod and compatible”

Recommendation: an explicit adapter registry must own compatibility. A same filename, same display name, shared loader, or semver major match does not prove schema compatibility.

| Evidence | Required decision |
| --- | --- |
| Canonical mod ID and implementation lineage | Verify installed metadata, not jar filename. Forks such as renderer replacements need distinct identities unless an adapter declares compatibility. |
| Minecraft release and loader family/version | Both must fall inside the adapter's tested combinations. Fabric/Forge/NeoForge builds can expose different files and semantics. |
| Exact mod version or declared compatible range | Start with exact version and preferably jar hash; broaden only with fixture or upstream schema evidence. Mod version ranges describe mod compatibility only if the adapter explicitly says so. |
| Config owner and relative path | Ownership must be known. A shared config library may serialize files for multiple mods; its identity is not the settings owner. |
| Schema ID/revision or structural fingerprint | Check key paths, value types, enum members, units, required fields, and explicit schema version. Matching structure is supporting evidence, not proof of equal meaning. |
| Assets/dependencies referenced | Resolve shaderpacks, resource packs, registry IDs, optional companion mods, and local device names. Missing references cause a skip, not copying more assets silently. |
| Migration rule | Record source and destination schema, direction, lossiness, supported bounds, and rollback requirements. No adapter means no automatic cross-version sync. |

Fabric's metadata identifies mods, entrypoints, dependencies, and versions; it does not promise config compatibility. [Fabric project/metadata guide](https://docs.fabricmc.net/develop/getting-started/project-structure). NeoForge's `CLIENT` role is a useful initial filter, while `COMMON` alone does not mean a preference is safe: common settings may affect gameplay. Server settings can be world-specific and network-synchronized; leave them outside the local preference feature. [NeoForge config types](https://docs.neoforged.net/docs/misc/config/).

Security/storage recommendations: resolve allowlisted relative paths beneath the actual game directory; reject path traversal and links escaping that directory. Do not sync entire `config/` trees, authentication files, tokens, account IDs, absolute paths, generated caches, world data, or server permissions. This is a field-selection feature, not general instance replication.

## 8. Launch/exit transactions and conflicts

### Normal transaction

Before launch, acquire an instance/game-directory lease and pin an immutable shared-profile revision. Confirm the actual game directory is not already used by another game process, even if a different launcher instance record points to it. Read current bytes and hashes, prepare an adapter-filtered field patch, and show/report skipped or adjusted values. Back up each touched file, write a sibling temporary file, flush it, recheck the original hash, and replace safely. Record a transaction journal with the preimage and the exact applied postimage. Multi-file changes need journaled recovery; an atomic rename of one file does not make the whole batch atomic.

The **post-application baseline**, not the original pre-sync file, is the baseline for detecting in-game edits. After the launcher-owned child process and relevant writers finish, take a stable read and semantic diff. Ignore ordering, float spelling, version stamps, generated defaults, and other known save normalization when identifying actual preference changes. Unsupported or ambiguous migrations remain observations, not shared updates. A crash or truncated file must not clear the profile.

### Conflict policy

| Scenario | Recommended initial behavior |
| --- | --- |
| User edits options in-game in a synced instance | Keep the saved instance file and capture a proposal relative to what Nexora applied. Offer to retain an instance override, explicitly publish selected changes, or reapply the shared profile next launch. Explain the next-launch behavior before it overwrites intentional local edits. |
| Two instances change the same preference differently | Store two proposals with instance/session IDs and base profile revisions. Never choose by mtime. Publishing requires a field-level decision against the current revision. |
| Two instances change different preferences | Keep separate proposals in v1. A later explicit publish may merge nonoverlapping fields using a three-way comparison. |
| Profile edited while game is running | Running session keeps its pinned revision. Compare its proposal's base with the current profile before accepting it; do not apply the new revision to a live process. |
| Game running when sync is requested | Defer all managed file writes until the game directory is idle. An OS file lock alone does not establish safety: Java may keep in-memory values and later overwrite disk changes. |
| External editor changes file between read and replacement | Hash mismatch aborts and recomputes the patch from fresh bytes. Do not overwrite the editor's work with the previously prepared whole document. |
| Game adds defaults, rewrites quotes, drops absent-mod keys, or migrates values | Classify as target normalization or migration where known. Do not interpret as user deletion/reset. Unknown changes remain reviewable observations. |
| File missing or malformed after exit | Retain profile and last good snapshot, surface an error, and skip publication. Do not guess that the user wants defaults everywhere. |
| New mod version changes its schema | Invalidate that adapter's applicability until compatibility is established. Other independent vanilla fields can still sync. |
| User deletes/reorders servers | Treat as an explicit ordered-list proposal only within the opted-in server module. Do not turn an absent or unreadable list into a global deletion. |

A three-way publish compares the applied semantic baseline, the after-exit observation, and the current shared profile. If the profile's field is unchanged since the pinned revision, an explicit acceptance can update it. If both sides changed the same field differently, retain both values for resolution. Deletion/reset must be an explicit operation distinct from absent, unsupported, skipped, and parse failure.

Recommendation: normal one-way mode enforces the shared selected fields on subsequent launches. A “retain locally” choice creates a per-instance exclusion/override, not an invisible second source of truth. Store proposals locally with bounded retention; no cloud and no account synchronization are necessary.

## 9. Recommended data model sketch

Use these conceptual records; this is intentionally prose and tables rather than product code.

| Record | Suggested fields | Purpose |
| --- | --- | --- |
| Shared profile | UUID, label, monotonic revision, category selections, semantic preference map, optional server module, creation/update times | User intent independent of one client's serialization. |
| Semantic preference | Stable Nexora ID, typed value, explicit-set/reset state, category, source provenance, optional backend/schema qualification | Distinguish absent from reset and preserve source context for conversions. |
| Keybinding value | Owning mod/vanilla identity, stable action ID, device kind, logical key or physical key/scancode identity, modifiers/context, source backend | Avoid conflating LWJGL integers, GLFW symbols, and SDL physical positions. Unsupported conversions remain unavailable. |
| Version capability record | Exact release ID, DataVersion, options schema family, key backend, verified defaults, supported fields/codecs, evidence URL and fixture hash | DataVersion alone does not define the complete settings schema. |
| Instance binding | Instance UUID, canonical game directory, chosen profile/revision policy, include/exclude categories, local overrides, loader/mod inventory | Resolve the effective intent without modifying the shared profile. |
| Adapter descriptor | Adapter version, owned file/path and fields, mod ID/lineage, game/loader/mod version constraints, schema signature, parser/codec, conversions, exclusions, verification status | An auditable compatibility decision rather than heuristic whole-file copying. |
| Local document snapshot | Original bytes/encoding, hash, parsed records/tree, unrecognized fields, source DataVersion, error status | Lossless patching, rollback, and meaningful diagnostics. Not a shared profile payload. |
| Launch transaction | UUID, instance/session/process identity, pinned profile revision, before/applied hashes and backups, semantic baseline, adapter versions, commit/recovery state | Recover interrupted multi-file writes and identify the true diff baseline. |
| Exit observation / proposal | Session ID, base revision, changed semantic values, before/after hashes, normalization notes, conflicts, acceptance state | Read-after-exit without automatic reverse synchronization. |
| Shared server entry | Launcher UUID, display name, original address, order | Optional portable intent; icons, unknown tags, and trust choices remain local. |

Keep separate schema-version numbers for Nexora's profile storage, each adapter, vanilla DataVersion, and mod-owned config schemas. Retain raw unknown values only in local snapshots. Reports should distinguish applied, unchanged, unsupported version, missing asset, excluded state, invalid value, and conflict; “sync succeeded” must not hide a skipped category.

## 10. Implementation checklist

- [ ] Confirm scope: same PC, one-way profile application, separate opt-in server list, no automatic post-exit publication.
- [ ] Establish a supported exact-version list; do not advertise all versions from 1.12 through 26.3 merely because the DataVersion table contains them.
- [ ] Generate clean and modified client fixtures in separate game directories for supported versions; record artifact IDs/hashes and fixture provenance.
- [ ] Resolve every applicable **UNVERIFIED** rule before enabling it, especially quoted strings, language/pack boundaries, legacy bindings, 1.21.11 graphics fields, and 26.3 input serialization.
- [ ] Exercise first load and later save independently: missing key, unknown key, duplicate key, malformed scalar/array, old/new `version`, absent mod, and unbound/non-US keys. Inspect bytes before load, after menu entry, after option change, and after exit.
- [ ] Verify migration behavior with fixtures; do not spoof `version`, run arbitrary game data-fixer code, or rely on implicit downgrade behavior.
- [ ] Implement typed per-key codecs and target capability rules with deliberate downgrade loss handling. Keep unrecognized text intact during Nexora's own patching.
- [ ] Define category ownership, including overlaps between vanilla graphics options and Sodium/OptiFine/Iris; prevent two adapters from writing competing values.
- [ ] Exclude window overrides, history, tutorials, warnings/consent, absent packs, telemetry choices, device names, debug state, fingerprints, and unknown mod fields by default.
- [ ] Make pack/shader selection separately opt-in; validate asset identity, availability, order, and target compatibility without copying assets implicitly.
- [ ] Gate mod settings by mod ID/lineage, game/loader/mod versions, schema, and owned fields; exclude server/world/content settings.
- [ ] Select an NBT crate after small-file correctness tests and release-license review. Test uncompressed named-root NBT, optional tags, unknown fields, tri-state pack permission, Unicode, IPv6 addresses, and malformed/oversized input.
- [ ] Implement exclusive game-directory coordination, process-aware deferral, stable reads, compare-before-replace hashes, backups, atomic per-file replacement, and multi-file recovery journaling.
- [ ] Store the applied semantic baseline and pinned revision before starting the game. Recover orphaned sessions after launcher/game crashes without publishing inferred deletions.
- [ ] Verify in-game edits, conflicting instance edits, external editor races, profile edits during play, and two instance records sharing one directory.
- [ ] Keep exit observations as proposals; require an explicit publish action and three-way field conflict handling. Document that unresolved changes are reapplied from the profile on the next launch unless locally overridden.
- [ ] Preserve dependency notices and write adapters independently from published format behavior and owned fixtures; do not copy GPL launcher implementations.
- [ ] Report unsupported/adjusted fields precisely and provide rollback. Run targeted round-trip and lifecycle tests before calling any adapter compatible.

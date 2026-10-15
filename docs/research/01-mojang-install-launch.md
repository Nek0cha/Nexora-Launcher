# Vanilla Minecraft Java: installation and launch reference

Research date: 2026-10-15. Target: Nexora Launcher, Rust / Tauri 2, Windows first, then Linux and macOS; planned Apache-2.0 license.

## Evidence and scope

This is a description of data and behavior, written independently; it contains no copied launcher implementation. Mojang metadata excerpts below are real, reduced JSON projections: omitted fields are not represented by invented JSON values. Recommendations for Nexora are explicitly described as policy. **UNVERIFIED** means a claim or compatibility detail was not established by the checks described here.

Live verification fetched the version catalog, **all 919 version JSONs then listed**, and every distinct referenced asset index over certificate-validated HTTPS using Python's standard library. Every version JSON matched its catalog SHA-1. Also checked three Java 21 platform manifests, three logging configurations, one asset object, one native JAR's contents, and one LZMA runtime download including its decompressed hash. No game or downloaded Java executable was run; actual launch compatibility remains **UNVERIFIED** on all three operating systems.

Requested curl checks were attempted, including `curl.exe -fsSL https://piston-meta.mojang.com/mc/game/version_manifest_v2.json`. They failed locally: the configured proxy pointed to an unavailable loopback service; direct curl then failed with Schannel `SEC_E_NO_CREDENTIALS`. The successful fallback used `urllib.request.build_opener(urllib.request.ProxyHandler({}))`, without disabling certificate validation. Consequently these are live HTTPS checks, **not successful curl checks**. Reproduce on a working curl installation:

```powershell
curl.exe -fSL https://piston-meta.mojang.com/mc/game/version_manifest_v2.json
curl.exe -fSL https://piston-meta.mojang.com/v1/packages/832d95b9f40699d4961394dcf6cf549e65f15dc5/1.12.2.json
curl.exe -fSL https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json
```

License provenance matters: the inspected [PortableMC v4.4.1 LICENSE](https://github.com/theorzr/portablemc/blob/v4.4.1/LICENSE) is **GPL-3.0**, despite PortableMC sometimes being suggested as a permissive example. Its behavior was consulted, but no code is reproduced. [XMCL core's LICENSE](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/LICENSE) is MIT. Treat licenses as specific to repository and revision; Nexora's license does not relicense Minecraft binaries or bundled Java distributions.

Sources: [live version catalog](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json), [runtime catalog](https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json), repository license links above.

## 1. Version catalog: `version_manifest_v2.json`

Root fields:

| Field | Meaning / handling |
|---|---|
| `latest.release`, `latest.snapshot` | Version IDs, resolved by looking up `versions`; do not construct a version URL from the ID. At verification these were `26.3` and `26.4-snapshot-4`. |
| `versions[]` | Catalog entries. Treat IDs as opaque strings; chronological or semantic-version sorting is not a dependency resolver. |
| `id`, `type`, `url` | Version identity, category, and authoritative version JSON URL. |
| `time` | Metadata update timestamp; can change long after the game's release. |
| `releaseTime` | Original release timestamp. |
| `sha1` | SHA-1 of the **downloaded version JSON bytes**, not the client JAR. Hash before parsing/reformatting. |
| `complianceLevel` | Observed values 0 and 1; preserve it. Exact launcher policy obligations are **UNVERIFIED** here; do not interpret it as Java version or authenticity level. |

Types are `release`, `snapshot`, `old_beta`, and `old_alpha`. Snapshot includes development builds such as pre-releases/release candidates; old categories identify historical Java Beta/Alpha, not current preview channels. Unknown future categories should remain representable.

Real catalog entry:

```json
{
  "id": "1.12.2",
  "type": "release",
  "url": "https://piston-meta.mojang.com/v1/packages/832d95b9f40699d4961394dcf6cf549e65f15dc5/1.12.2.json",
  "time": "2023-06-07T11:49:20+00:00",
  "releaseTime": "2017-09-18T08:39:46+00:00",
  "sha1": "832d95b9f40699d4961394dcf6cf549e65f15dc5",
  "complianceLevel": 0
}
```

Nexora policy: cache metadata with provenance and refresh support; verify size/hash wherever provided, download to a temporary sibling and atomically publish after verification. SHA-1 is an integrity identifier, not a signature: retain HTTPS verification. Reject paths escaping managed roots, including version IDs used as directory names. Never infer artifact hosts when an explicit URL exists.

Source: [Mojang catalog](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json).

## 2. Version JSON

### Identity, inheritance, entry point, downloads

`id` identifies the profile. `type`, `time`, and `releaseTime` parallel catalog concepts. `minimumLauncherVersion` is a historical Mojang launcher capability marker, not a minimum Nexora or Java version. `mainClass` is the Java class invoked after JVM arguments: commonly `net.minecraft.client.main.Main`; historical profiles can specify `net.minecraft.launchwrapper.Launch`. Always use metadata, including historical wrapper libraries, rather than a hardcoded entry point.

`downloads.client` supplies `{url, sha1, size}` for the game JAR. A conventional local layout is `versions/<id>/<id>.json` and `versions/<id>/<id>.jar`. Other download keys such as server and mappings are not needed to play the client. `assetIndex` supplies `{id, url, sha1, size, totalSize}`: `size` describes index bytes, `totalSize` describes asset payload, and `id` names the local index. `assets` is an older asset-set identifier; prefer explicit `assetIndex` data.

No `inheritsFrom` occurred in the 919 official version JSONs checked. It is relevant to custom/modded profiles, not a prerequisite for installing the current vanilla catalog. Resolve its referenced parent recursively, with cycle/depth limits. There is no verified universal Mojang merge specification for third-party profiles. **UNVERIFIED as official behavior**; recommended Nexora compatibility policy, informed by existing resolvers:

1. Resolve oldest parent first. Child-defined scalar fields override parent fields; absent fields inherit. Child `id` remains the selected profile ID.
2. Merge nested objects by key with child precedence, retaining omitted download keys. Treat structured selections such as `assetIndex` and `javaVersion` coherently rather than accidentally combining incompatible halves.
3. Concatenate modern `arguments.jvm` and `arguments.game` parent first, child second. Retain argument order and conditional entries. Legacy `minecraftArguments` is a complete string override when supplied by the child, not an array to concatenate.
4. Combine libraries with child precedence for conflicts identified by Maven group, artifact, classifier, and extension, excluding version from that conflict key. Preserve classifiers: native and ordinary artifacts must not replace one another. Retain deterministic order. This is a launcher convention, not a promise that arbitrary loader profiles agree.
5. Use the ancestor that supplies `downloads.client` as the client-JAR owner unless an explicitly supported custom `jar` selector changes it. Do not expect a child with no download to have its own JAR. Keep selected profile identity distinct from JAR ownership.

Mixed legacy/modern argument inheritance and loader-specific extension fields need a separately tested compatibility contract; **UNVERIFIED**, outside this vanilla reference.

Real projection from 1.12.2:

```json
{
  "id": "1.12.2",
  "mainClass": "net.minecraft.client.main.Main",
  "javaVersion": {"component": "jre-legacy", "majorVersion": 8},
  "downloads": {
    "client": {
      "sha1": "0f275bc1547d01fa5f56ba34bdc87d981ee12daf",
      "size": 10180113,
      "url": "https://piston-data.mojang.com/v1/objects/0f275bc1547d01fa5f56ba34bdc87d981ee12daf/client.jar"
    }
  }
}
```

Sources: [1.12.2 JSON](https://piston-meta.mojang.com/v1/packages/832d95b9f40699d4961394dcf6cf549e65f15dc5/1.12.2.json), [1.5.2 historical entry point](https://piston-meta.mojang.com/v1/packages/924a2dcd8bdc31f8e9d36229811c298b3537bbc7/1.5.2.json), [MIT XMCL version resolver](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/packages/core/version.ts).

### Libraries: ordinary artifacts and two native formats

Each `libraries[]` entry has a Maven-style `name`, optionally `rules`, and download information. Prefer `downloads.artifact.path` under `libraries/` and its explicit `url`, `sha1`, `size`. For a legacy entry genuinely lacking download descriptors, the conventional coordinate `group:artifact:version[:classifier][@extension]` maps to `group/with/slashes/artifact/version/artifact-version[-classifier].extension`, default extension `jar`, under its `url` repository or `https://libraries.minecraft.net/`. Such fallback lacks a verified digest unless separately supplied; do not silently invent one.

**Older native format:** evaluate the library rules, then select `natives[os]` for `windows`, `linux`, or `osx`. Expand `${arch}` to `32` or `64` according to the **Java process** architecture, then look up that exact key in `downloads.classifiers`. Download/extract only the selected native descriptor. The same entry may also have an ordinary `downloads.artifact`; handle that separately for the classpath. A classifiers-only native entry does not imply a base JAR exists. `extract.exclude` gives archive-path prefixes to skip, commonly `META-INF/`. A missing OS mapping means no native selection for that OS.

Real 1.8.9 projection illustrating the separate bitness substitution:

```json
{
  "name": "tv.twitch:twitch-platform:6.5",
  "natives": {
    "linux": "natives-linux",
    "osx": "natives-osx",
    "windows": "natives-windows-${arch}"
  },
  "extract": {"exclude": ["META-INF/"]},
  "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "linux"}}]
}
```

**Newer native format:** 1.19 and later examples list native classifiers as independent library entries whose `downloads.artifact` is itself the native JAR, controlled by OS rules. These are **classpath artifacts**, even when their Maven name ends in `natives-windows`. LWJGL 3 can locate and extract bundled resources itself. Do not require a `natives` map, or remove every native-looking artifact from the classpath.

```json
{
  "name": "org.lwjgl:lwjgl:3.3.1:natives-windows",
  "downloads": {
    "artifact": {
      "path": "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar",
      "sha1": "0036c37f16ab611b3aa11f3bcf80b1d509b4ce6b",
      "size": 159361,
      "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
    }
  },
  "rules": [{"action": "allow", "os": {"name": "windows"}}]
}
```

Sources: [1.8.9 JSON](https://piston-meta.mojang.com/v1/packages/d546f1707a3f2b7d034eece5ea2e311eda875787/1.8.9.json), [1.19 JSON](https://piston-meta.mojang.com/v1/packages/14bbfb25fb1c1c798e3c9b9482b081a78d1f3a9d/1.19.json), [LWJGL native loading guidance](https://github.com/LWJGL/lwjgl3#loading-lwjgl).

### Rules

For a nonempty rule list, start disallowed, examine entries in order, and replace the decision with each **matching** entry's `action` (`allow` or `disallow`). Thus the last matching rule wins. No `rules` means unconditional inclusion. Empty lists are not exercised by the observed examples; launchers differ: **UNVERIFIED** official empty-list behavior. Nexora policy should explicitly choose and test it (recommended: empty means unconditional, matching XMCL).

All conditions within a rule must match; omitted conditions impose no restriction:

- `os.name`: exact launcher names `windows`, `linux`, `osx`, not Rust platform names `macos` or `darwin`.
- `os.arch`: architecture predicate. The entire catalog scan found only literal `x86`; this is distinct from the `${arch}` bitness substitution. Use the selected JVM's architecture, normalizing host labels such as `ia32` appropriately. Do not blindly map every 64-bit CPU to the same architecture. A universal regex interpretation of `os.arch` is **UNVERIFIED**; observed launcher resolvers compare architecture names, whereas `os.version` is regex-based.
- `os.version`: regex against OS version, using a search/match that permits prefix patterns such as `^10\.`; not equality or an implicit full-string match. Another real pattern is `^10\.5\.\d$`. Do not compare against the launcher version. Exact cross-platform official OS-version string formatting is **UNVERIFIED**; preserve build information and test predicates per platform.
- `features`: every named Boolean must equal the launch context's value; missing context features default false, so a requirement of false matches a missing feature. Known features: `is_demo_user`, `has_custom_resolution`, `has_quick_plays_support`, `is_quick_play_singleplayer`, `is_quick_play_multiplayer`, `is_quick_play_realms`.

**Current-format addition:** 26.3 has `os.versionRange` in `arguments.default-user-jvm`, e.g. `{"min":"10.0.17134"}` or `{"max":"10.0.17134"}`. It is not a regex. Exact endpoint inclusivity and missing-component comparison semantics are **UNVERIFIED**; do not silently ignore this predicate and enable mutually incompatible garbage collectors. Until verified, Nexora can supply its own explicit, supported user JVM defaults and retain mandatory `arguments.jvm`.

Sources: [MIT XMCL rule evaluator](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/packages/core/version.ts), [PortableMC rule behavior, GPL reference only](https://github.com/theorzr/portablemc/blob/v4.4.1/portablemc/standard.py), [26.3 JSON](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json).

### Arguments and placeholder values

For modern profiles, independently evaluate `arguments.jvm` and `arguments.game`. Each element is either a string (one argument) or `{rules, value}`, where `value` is one string or an ordered array of strings. Include allowed values, flatten one level, then substitute placeholders **within each argument**. Never split substituted paths, names, JSON, or tokens on spaces; never invoke a shell. Reject an unresolved placeholder in an included argument with a useful error. Excluded arguments need no values.

For older profiles, tokenize `minecraftArguments` **before substitution**, respecting any template quoting; its historical vanilla examples mainly use whitespace-separated tokens. Do not feed it through `cmd.exe`, PowerShell, or a Unix shell. Supply the missing JVM scaffolding yourself: memory settings, `-Djava.library.path=<native-dir>`, `-cp`, one joined classpath, relevant platform flags, and metadata logging configuration. Modern argument arrays are used by release 1.13 onward; 1.12.2 demonstrates the legacy string. Select by field presence, not a version-number comparison.

The complete placeholder set found across the 919 downloaded JSONs is covered below. Values describing identity assume a valid Minecraft-authenticated session; authentication acquisition is outside this document.

| Placeholder(s), including `${...}` syntax | Value |
|---|---|
| `${auth_player_name}` | Minecraft profile name, not Microsoft account email. |
| `${auth_uuid}` | Minecraft profile UUID, conventionally 32 hexadecimal digits without hyphens. |
| `${auth_access_token}` | Minecraft Services access token, not Microsoft OAuth, Xbox Live, or XSTS token. |
| `${auth_session}` | Historical session representation, conventionally `token:<Minecraft-access-token>:<profile-uuid>` for authenticated legacy clients. Do not substitute that compound value for modern `auth_access_token`. Actual historical online session interoperability is **UNVERIFIED**. |
| `${user_type}` | `msa` for Microsoft sessions; historical values `mojang` and `legacy` describe older account types. Only pass where requested; current 26.3 omits it. |
| `${user_properties}` | Compact JSON property map; `{}` when no legacy properties are supplied. It is a single argument. |
| `${auth_xuid}` | Xbox user ID from the authenticated account/session, decimal string; not UUID or user hash. |
| `${clientid}` | Authentication integration's client identifier. Exact official mapping to OAuth application ID versus launcher/session client ID is **UNVERIFIED**; existing launcher integrations differ. Define this in Nexora's auth contract rather than using another launcher's identifier or a token. |
| `${version_name}`, `${version_type}` | Selected effective profile ID and type; vanilla examples are `1.20.1`, `release`. |
| `${game_directory}` | Absolute instance/game working directory. |
| `${assets_root}` | Absolute shared `assets/` root containing `indexes/` and `objects/`. |
| `${assets_index_name}` | `assetIndex.id`, e.g. `1.12`, `5`, `24`; local index is `assets/indexes/<id>.json`. Not necessarily the Minecraft version. |
| `${game_assets}` | Legacy expanded asset directory: `assets/virtual/<index-id>` for a virtual index; for pre-1.6 resource mapping use the instance's `resources/` view when needed. Physical resource mapping must exist regardless of the wrapper's argument handling. |
| `${natives_directory}` | Absolute per-launch native/extraction directory. Preserve subpaths in JVM templates; 26.3 uses `/java`, `/jna`, `/lwjgl`, `/netty`. |
| `${launcher_name}`, `${launcher_version}` | Nexora's brand (`Nexora`) and actual launcher version. |
| `${classpath}` | Complete ordered classpath joined with target OS separator, one argument. |
| `${resolution_width}`, `${resolution_height}` | Positive requested window dimensions as decimal strings, only when custom-resolution feature is true. |
| `${quickPlayPath}` | Launcher-selected quick-play log path; relative paths resolve from the game working directory. |
| `${quickPlaySingleplayer}` | Save directory name/identifier, not its display title or absolute path. |
| `${quickPlayMultiplayer}` | Server address, optionally with port, e.g. `localhost:25565`. |
| `${quickPlayRealms}` | Realm ID string. |
| `${arch}` | Native classifier context only: `32` or `64`, not `x86_64` or `arm64`. |
| `${path}` | Logging-argument context only: absolute downloaded logging-config path. |

Common third-party extensions `${library_directory}` (absolute `libraries/`) and `${classpath_separator}` (`;` or `:`) were **not present in the 919 official JSONs** but are supported by existing resolvers. Other unknown extensions require explicit handling rather than empty substitution. Token values must be redacted from command previews, logs, crash attachments, and error messages.

26.3 also includes `arguments.default-user-jvm`, a separate set of conditional recommended memory/GC defaults. Treat it as defaults configurable by the user, not a replacement for `arguments.jvm`; avoid duplicate competing heap or collector options. It includes `-Xms2G`, `-Xmx4G`, and conditional ZGC/G1 choices. Mandatory JVM options have also evolved: 26.3 includes native-access and export flags. Preserve metadata instead of freezing an older JVM template.

Sources: [1.12.2 arguments](https://piston-meta.mojang.com/v1/packages/832d95b9f40699d4961394dcf6cf549e65f15dc5/1.12.2.json), [1.20.1 arguments](https://piston-meta.mojang.com/v1/packages/a5cf661d31d9dfcd3d1645be80f43954f63f63b0/1.20.1.json), [26.3 arguments](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json), [MIT XMCL argument construction](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/packages/core/launch.ts), [PortableMC authentication conventions, GPL reference only](https://github.com/theorzr/portablemc/blob/v4.4.1/portablemc/auth.py), [Mojang 26.1 JVM changes](https://www.minecraft.net/en-us/article/minecraft-java-edition-26-1).

### Java requirement and logging configuration

`javaVersion.component` selects a Mojang runtime catalog component; `majorVersion` is the required Java major, not the patch level. Examples: 1.16.5 → `jre-legacy`/8; 1.17 → alpha/16; 1.18 → beta/17; 1.19 and 1.20.1 → gamma/17; 1.21.5 → delta/21; 26.3 → epsilon/25. Resolve the actual field rather than a permanently hardcoded version table. Some old JSONs, e.g. 1.6.4, omit it; Java 8 is a launcher compatibility default, with actual per-version behavior **UNVERIFIED** here. A user-supplied Java must match major, CPU architecture, and native requirements; newer Java is not automatically compatible with old games.

`logging.client` contains `type`, `argument`, and `file:{id,url,sha1,size}`. For `log4j2-xml`, download/verify the file, conventionally to `assets/log_configs/<id>`, substitute its absolute path into the logging argument, and append that as a **JVM** argument before the main class. Do not substitute the logging `${path}` globally.

Real 1.12.2 logging object:

```json
{
  "client": {
    "argument": "-Dlog4j.configurationFile=${path}",
    "file": {
      "id": "client-1.12.xml",
      "sha1": "bd65e7d2e3c237be76cfbef4c2405033d7f91521",
      "size": 888,
      "url": "https://piston-data.mojang.com/v1/objects/bd65e7d2e3c237be76cfbef4c2405033d7f91521/client-1.12.xml"
    },
    "type": "log4j2-xml"
  }
}
```

Sources: [1.16.5 JSON](https://piston-meta.mojang.com/v1/packages/fba9f7833e858a1257d810d21a3a9e3c967f9077/1.16.5.json), [1.21.5 JSON](https://piston-meta.mojang.com/v1/packages/b341cd0e91419ab72b5d9dadad8d86a045d6b9a7/1.21.5.json), [26.3 JSON](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json), [actual 1.12 logging XML](https://piston-data.mojang.com/v1/objects/bd65e7d2e3c237be76cfbef4c2405033d7f91521/client-1.12.xml).

## 3. Assets and historical directory views

The asset index is an object whose `objects` map associates logical resource paths with `{hash,size}`. `hash` is the object's SHA-1. Store an object at `assets/objects/<first-two-hash-characters>/<whole-hash>` and fetch it from `https://resources.download.minecraft.net/<first-two>/<hash>`. Several logical paths may share one hash; deduplicate downloads but preserve every name when materializing a view. Verify both byte size and digest.

Real projection from asset index 5:

```json
{
  "objects": {
    "icons/icon_128x128.png": {
      "hash": "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356",
      "size": 9101
    }
  }
}
```

The corresponding [object](https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356) was fetched and matched both values.

| Index flags | Required additional view | Verified version examples |
|---|---|---|
| Neither flag | Keep hash store; game uses the index. | 1.7.3, 1.7.10, 1.12.2, modern versions. |
| `virtual: true` | Copy/link each object to `assets/virtual/<index-id>/<logical-path>`; pass that root as `game_assets`. | `legacy` index: release 1.6.1, 1.6.2, 1.6.4 **and 1.7.2**; also sampled late-2013 snapshots such as 13w48b. |
| `map_to_resources: true` | Copy/link each object to `<game-directory>/resources/<logical-path>`. | `pre-1.6` index: releases through 1.5.2 in the catalog; Beta b1.7.3 and Alpha a1.2.6 verified. |

Follow the flags, not these release boundaries; snapshot chronology is irregular. The complete distinct-index scan found only these two flagged indexes. If both flags occur in future metadata, Nexora policy is to build both views. Validate logical paths against traversal and absolute paths. On Windows, copies or hard links avoid assuming symlink privileges. Views must not allow an instance to mutate the shared object store; copying is the simplest isolation policy.

Sources: [index 5](https://piston-meta.mojang.com/v1/packages/50ce48459eb9e29836662862273d7b15d41c2c33/5.json), [legacy index](https://launchermeta.mojang.com/v1/packages/770572e819335b6c0a053f8378ad88eda189fc14/legacy.json), [pre-1.6 index](https://launchermeta.mojang.com/v1/packages/3d8e55480977e32acd9844e545177e69a52f594b/pre-1.6.json), [1.7.2 metadata](https://piston-meta.mojang.com/v1/packages/c2e8ecbf355760a74c93d7210767fa043d53f27c/1.7.2.json).

## 4. Mojang Java runtime catalogs and file manifests

The known runtime catalog is available on both [launchermeta](https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json) and [piston-meta](https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json). The identifier in this URL is not the SHA-1 of a permanently immutable catalog. Keep the bootstrap URL configurable.

Root keys observed: `gamecore`, `linux`, `linux-i386`, `mac-os`, `mac-os-arm64`, `windows-arm64`, `windows-x64`, `windows-x86`. `linux` denotes the supplied x64 distribution; no Linux ARM key was present. Runtime OS keys differ from library rule names (`mac-os` versus `osx`). Select for the Java process architecture, including any deliberately chosen emulation mode.

Each platform maps component names to **arrays** of candidates; an empty array means unavailable. Observed components: `jre-legacy` (Java 8), `java-runtime-alpha` (16), beta/gamma/gamma-snapshot (17), delta (21), **epsilon (25)**, plus `minecraft-java-exe` (a separate launcher helper, not a Java major). Preserve unknown components. On this check macOS ARM lacked alpha, beta, and legacy; Windows x86 lacked delta and epsilon. Do not index candidate zero without checking availability.

Candidates contain `availability:{group,progress}`, `manifest:{url,sha1,size}`, and `version:{name,released}`. Availability is rollout metadata; exact official group assignment/selection is **UNVERIFIED**. Nexora policy: choose a fully available compatible candidate, verify the actual executable's major/architecture, or report that an external compatible Java is required. Do not arbitrarily substitute components.

Real `windows-x64.java-runtime-delta[0]`:

```json
{
  "availability": {"group": 312, "progress": 100},
  "manifest": {
    "sha1": "cb4394a27089d19f65d5baa6cf0482c27c3c7865",
    "size": 148529,
    "url": "https://piston-meta.mojang.com/v1/packages/cb4394a27089d19f65d5baa6cf0482c27c3c7865/manifest.json"
  },
  "version": {"name": "21.0.7", "released": "2025-05-19T08:34:42+00:00"}
}
```

The downloaded manifest has a root `files` map keyed by relative installation path. Observed file types:

- `directory`: create the directory, including empty ones.
- `file`: `downloads.raw` describes installed bytes; optional `downloads.lzma` describes a compressed alternative. Each has its own URL, SHA-1, and size. Verify the compressed payload, decode LZMA, then verify decoded bytes against **raw** hash/size. Raw URLs are a valid simpler fallback. The sampled compressed URL ended in `java.exe`, not `.lzma`: use metadata, not suffix detection. Set executable permission when `executable:true` on Unix; Windows uses normal executable-file semantics.
- `link`: create the relative symbolic link specified by `target`, relative to the link's containing directory. Validate the resolved target remains inside the runtime root. Create links after targets; a Windows fallback can copy resolved targets with cycle checks if symlinks are unavailable.

Real projections from the Windows and Linux delta manifests:

```json
{
  "files": {
    "bin": {"type": "directory"},
    "bin/java.exe": {
      "type": "file",
      "executable": true,
      "downloads": {
        "lzma": {
          "sha1": "b4d610f2cc9d12d2b9907622eecdc1833d441551",
          "size": 14407,
          "url": "https://piston-data.mojang.com/v1/objects/b4d610f2cc9d12d2b9907622eecdc1833d441551/java.exe"
        },
        "raw": {
          "sha1": "38dc560c6ff23b550a9df5b79a7de0d852e6f840",
          "size": 39424,
          "url": "https://piston-data.mojang.com/v1/objects/38dc560c6ff23b550a9df5b79a7de0d852e6f840/java.exe"
        }
      }
    }
  }
}
```

```json
{
  "files": {
    "legal/java.compiler/LICENSE": {"target": "../java.base/LICENSE", "type": "link"}
  }
}
```

Typical Java executable paths relative to the runtime root: Windows `bin/java.exe` (or `javaw.exe` for a windowless process), Linux `bin/java`, macOS `jre.bundle/Contents/Home/bin/java`. Keep the full runtime tree, not just the executable. Install atomically by platform/component/manifest identity and avoid changing a runtime used by an active game. Retain supplied license files.

Sources: runtime catalogs above; verified [Windows delta manifest](https://piston-meta.mojang.com/v1/packages/cb4394a27089d19f65d5baa6cf0482c27c3c7865/manifest.json), [Linux delta manifest](https://piston-meta.mojang.com/v1/packages/06a3884df3d9d4072b9b6df541495d81878ad2de/manifest.json), [macOS delta manifest](https://piston-meta.mojang.com/v1/packages/4a1d0bff88393d67292b2836efbb2d257df44a6a/manifest.json).

## 5. Classpath, natives, and command-length limits

Build the classpath from rule-allowed ordinary `downloads.artifact` JARs in metadata order, including newer standalone native JARs, followed by the selected client JAR. Legacy `downloads.classifiers` native selections go to extraction, not the classpath. Deduplicate exact paths stably; do not sort arbitrarily or replace the explicit list with a wildcard. `-cp` and the joined classpath are two separate arguments. Launch the specified class, not `java -jar client.jar`.

| Target | Classpath separator | Native examples / requirements |
|---|---|---|
| Windows | `;` | `.dll`; match Java bitness/CPU. Respect metadata's Windows-specific JVM options. |
| Linux | `:` | `.so`; Java architecture and system ABI must match. Mojang catalog availability is not a promise of musl compatibility. |
| macOS | `:` | `.dylib`, historical `.jnilib`; retain conditional `-XstartOnFirstThread` where supplied for LWJGL 3. Match Intel versus ARM Java. |

For legacy classifiers, extract to a fresh per-launch directory and honor `extract.exclude` prefixes. Reject absolute/drive/UNC paths, traversal, escaping symlinks, and unreasonable expansion sizes. Avoid mixing natives from different games, Java architectures, or concurrent launches. Expose the directory through metadata JVM arguments / legacy `java.library.path`; retain it until the process has exited, especially because Windows locks loaded DLLs.

For modern LWJGL artifacts, retaining JARs on the classpath permits architecture-aware resource loading and extraction. The inspected Windows 3.3.1 JAR contains `windows/x64/org/lwjgl/lwjgl.dll`; it is **not** a flat root DLL archive. If manually extracting modern resources, preserve the expected layout or use a separately verified loader configuration. Do not flatten Intel and ARM libraries together. Honor metadata's JNA/LWJGL/Netty extraction properties and create their directories.

Windows `CreateProcessW` has a 32,767-character command-line ceiling including its terminating NUL. `cmd.exe` additionally has a smaller limit (commonly 8,191 characters); avoid the shell altogether. Rust's argument API handles Windows quoting but cannot remove the OS ceiling. Count the serialized UTF-16 command length with escaping overhead, not merely UTF-8 argument lengths.

- **Java 9+:** `java @<argfile>` is supported. The file can hold JVM options, classpath, main class, and game arguments in order. Follow Java's own argfile quoting/escaping/comment rules, including backslashes in Windows paths; shell quoting is not the same grammar. Use absolute paths and a controlled encoding supported by the selected runtime. Relative paths resolve from process working directory, not the argfile's directory. Protect/delete files containing tokens after exit; do not expose them through debug logs.
- **Java 8:** the `java` launcher does **not** have Java 9's general `@argfile` support (other JDK tools having it does not establish support in `java`). Use short managed paths and, if necessary, a generated classpath-only JAR with manifest `Class-Path` entries and launch `-cp <classpath-jar> <mainClass> ...`. Manifest entries are space-separated URLs relative to that JAR, not OS-separated filesystem paths: encode spaces/non-ASCII correctly and apply manifest line folding. Keep the original classpath order. This fallback and Unicode behavior need integration tests: **UNVERIFIED in Nexora**. It only compresses classpath length, not arbitrarily long remaining arguments.

Sources: [LWJGL loading](https://github.com/LWJGL/lwjgl3#loading-lwjgl), [inspected native JAR](https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar), [Microsoft CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw), [Java 9 argfiles](https://docs.oracle.com/javase/9/tools/java.htm), [Java 9 enhancement announcement](https://www.oracle.com/java/technologies/javase/9-enhancements.html), [Java 8 launcher](https://docs.oracle.com/javase/8/docs/technotes/tools/windows/java.html), [Java 8 JAR manifest specification](https://docs.oracle.com/javase/8/docs/technotes/guides/jar/jar.html).

## 6. Process lifecycle, output, crashes, and Quick Play

Recommended launch sequence: ensure downloads and views are complete → select/validate Java → create private native directories → evaluate/expand JVM and game arguments → spawn `<absolute-java> <JVM-options> <logging-option> <mainClass> <game-options>` with the **instance game directory as `cwd`**. Pass the same directory in `--gameDir` where metadata requests it. Historical versions and logging configs may use `cwd` even when another directory argument exists.

Use Rust's process API with explicit executable and argument vector; create no shell command. Pipe stdout and stderr and drain both concurrently to avoid deadlocks. Keep the child handle and wait asynchronously; its completion/status is the authoritative exit signal. Do not infer exit from a window disappearing, a particular log line, or stream EOF alone. Reap the process and record exit code/signal plus elapsed time. A nonzero exit is diagnostic evidence, not a unique crash classification; launch failures and forced termination also produce it. Tauri UI shutdown and intentional game termination should have explicit policies.

Metadata logging configs direct stdout to Log4j XML events (`XMLLayout` in the 1.7 config, `LegacyXMLLayout` in the 1.12 and newer examples). Expect event fragments such as `log4j:Event` with logger, timestamp, level, thread, message/CDATA and optional throwable, mixed with plain JVM/native output. Buffer across reads and lines; do not assume one event per line or a single valid root XML document. Disable DTD/external-entity processing, bound buffers, preserve plain-text fallback, and redact tokens. Exact event variants for every bundled Log4j release are **UNVERIFIED**; the configs establish XML layout selection, not a universal wire schema.

The inspected configs write `logs/latest.log` and rotated `logs/<date>-<index>.log.gz` relative to `cwd`. Minecraft crash reports normally appear under `<game-directory>/crash-reports/`; JVM fatal errors typically produce `hs_err_pid*.log`, with location affected by JVM options and working directory. Detect newly created files after exit rather than assuming every abnormal termination produces a report. Actual crash-output behavior per historical version is **UNVERIFIED by execution** in this research.

Quick Play was added in **23w14a**, released in **1.20**. Current metadata has conditional game argument pairs:

| Feature | Argument and value |
|---|---|
| `has_quick_plays_support` | `--quickPlayPath ${quickPlayPath}`; requests a session/world log, relative to `cwd` if relative. |
| `is_quick_play_singleplayer` | `--quickPlaySingleplayer ${quickPlaySingleplayer}`; save folder identifier. |
| `is_quick_play_multiplayer` | `--quickPlayMultiplayer ${quickPlayMultiplayer}`; e.g. `example.org:25565`. |
| `is_quick_play_realms` | `--quickPlayRealms ${quickPlayRealms}`; Realm ID. |

Set at most one destination feature, supply its value, and enable logging separately when desired. Validate the requested save is within the instance. Prefer detecting these entries in metadata over parsing version numbers. 1.19.4 lacks them. Earlier supported clients can use historical `--server <host> --port <port>` for multiplayer; Mojang removed those options in 23w14a. Do not inject modern Quick Play into old versions or promise historical singleplayer equivalents.

Sources: [MIT XMCL process launch](https://github.com/Voxelum/minecraft-launcher-core-node/blob/master/packages/core/launch.ts), [actual logging config](https://piston-data.mojang.com/v1/objects/bd65e7d2e3c237be76cfbef4c2405033d7f91521/client-1.12.xml), [Mojang 23w14a argument definitions](https://www.minecraft.net/en-us/article/minecraft-snapshot-23w14a), [1.20.1 metadata](https://piston-meta.mojang.com/v1/packages/a5cf661d31d9dfcd3d1645be80f43954f63f63b0/1.20.1.json).

## 7. Compatibility and security pitfalls

- **ARM macOS:** 1.19 metadata includes both `natives-macos` and `natives-macos-arm64` under the same `osx` rules. Keep both classpath artifacts; LWJGL selects resources for the JVM architecture. An ARM runtime alone does not make old Intel-only natives compatible. Old versions may need an Intel Java plus Rosetta, or an explicitly managed native-library override. The current catalog's missing ARM Java 8/16/beta components confirms a distribution gap; actual Rosetta compatibility for each game version is **UNVERIFIED** here. [1.19 metadata](https://piston-meta.mojang.com/v1/packages/14bbfb25fb1c1c798e3c9b9482b081a78d1f3a9d/1.19.json), [runtime catalog](https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json).
- **Historical LWJGL/JNA:** do not silently replace LWJGL 2 with LWJGL 3, or assume the latest GLFW/JNA is compatible with old Java-side bindings. Other launchers report native crashes in old macOS clients and older JNA assertions. Keep any override separately versioned and opt-in, with a compatibility matrix. The reports establish observed failures, not universal fixes; Nexora reproduction is **UNVERIFIED**. [Prism old-macOS native crash report](https://github.com/PrismLauncher/PrismLauncher/issues/687), [Prism JNA report](https://github.com/PrismLauncher/PrismLauncher/issues/3446).
- **Windows non-ASCII and long paths:** using Unicode process APIs does not guarantee every historical JVM/native library handles all paths. Exercise Japanese user profiles, spaces, non-Latin instance names, and paths beyond conventional MAX_PATH, including Java 8. A native/extraction cache in a short ASCII path can be an explicit compatibility fallback; it does not prove the game directory works. Do not use lossy string conversion, shell escaping, or assume `-Dfile.encoding=UTF-8` repairs native filesystem handling. Exact affected JVM/library combinations and any universal workaround are **UNVERIFIED**. A launcher-specific non-Latin-name crash was fixed in [Prism 8.3](https://www.prismlauncher.org/news/release-8.3/); that does not establish the cause of every Unicode failure.
- **Log4Shell:** fresh official version metadata/logging configs are part of installation correctness. The fetched `client-1.12.xml` contains `%msg{nolookups}`; `client-1.7.xml` contains a regex filter rejecting lookup-shaped messages. Download and use the referenced config even for old versions; do not keep indefinitely stale metadata or replace it with a generic XML file. Mojang's December 2021 notice distinguishes clients from server mitigation instructions. Its server advice uses a flag for 1.17, one XML for 1.12–1.16.5, and a different XML for 1.7–1.11.2; do not substitute that table for current client metadata or apply one flag to every Log4j version. This research does not prove all historical configurations free from every later vulnerability. [Mojang notice](https://www.minecraft.net/en-us/article/important-message--security-vulnerability-java-edition), [client 1.12 XML](https://piston-data.mojang.com/v1/objects/bd65e7d2e3c237be76cfbef4c2405033d7f91521/client-1.12.xml), [client 1.7 XML](https://launcher.mojang.com/v1/objects/50c9cc4af6d853d9fc137c84bcd153e2bd3a9a82/client-1.7.xml).
- **Linux ABI:** OS/CPU labels alone do not establish compatibility with musl or available graphics/audio libraries. Test the chosen runtime and native set on supported distributions. Other launchers have recorded musl incompatibility; no general Nexora solution was verified. [Prism native ABI report](https://github.com/PrismLauncher/PrismLauncher/issues/4422).

## Implementation checklist

- Resolve IDs through fresh catalog data; verify metadata and every supplied artifact digest/size before atomic publication.
- Parse both argument formats; evaluate ordered rules and context-specific placeholders; reject unresolved included values and unsupported predicates.
- Handle legacy native classifiers and modern native artifacts separately; retain deterministic classpath order and validate extraction paths.
- Store asset objects once; materialize virtual/resource views from index flags, including the 1.7.2 exception.
- Select runtime by metadata component, major, OS, and JVM architecture; support raw/LZMA, executable bits, directories, and safe links.
- Include verified logging config and all mandatory platform JVM arguments; keep configurable defaults distinct.
- Spawn without a shell, use instance `cwd`, drain both streams, redact secrets, wait/reap, then clean per-launch resources.
- Use Java 9+ argfiles with correct grammar; validate the Java 8 classpath-JAR fallback and Windows Unicode/length behavior.
- Gate Quick Play by metadata capability and test only one destination at a time.
- Before claiming support, run Windows tests for modern, 1.12.2, 1.7.2, 1.6.4, and pre-1.6; later test Linux and Intel/ARM macOS separately. Resolve the marked **UNVERIFIED** auth-client-ID, rule-range, and execution details against the supported runtime/version matrix.

Checklist sources: the Mojang metadata, platform documentation, and compatibility reports linked in the corresponding sections above; checklist ordering and test matrix are Nexora implementation policy.

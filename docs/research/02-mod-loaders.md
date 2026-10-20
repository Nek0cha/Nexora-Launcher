# Official mod-loader metadata and installation

Research date: **2026-10-15**. Scope: Minecraft Java Edition client installation for Nexora Launcher (Rust, Tauri 2; planned Apache-2.0). This is a protocol and behavior study, not an implementation.

**Recommendation:** use official Fabric/Quilt profile APIs and a shared vanilla inheritance resolver. For Forge/NeoForge, interpret the selected official installer's metadata and run its declared Java processors in child processes. Start Forge support at **Minecraft 1.13.2**, with explicitly tested builds; add the separate legacy family later. An official installer subprocess is a viable fallback, but needs a launcher-compatible staging directory.

## Evidence, boundaries, and reproduction

All endpoint and artifact checks below used **curl against official hosts**, with redirects followed and TLS certificate validation enabled. Initial attempts failed because of an unavailable localhost proxy and the installed Schannel curl's credential error. A portable curl from the [official curl Windows distribution](https://curl.se/windows/) worked with a PEM bundle exported from the Windows certificate stores. No TLS verification bypass was used. Those environment failures are not loader endpoint failures.

Scratch root: `%TEMP%\nexora-loaders-f87170d95e1248d294a37aebed6ed7e1`. Each of nine installer JARs was unpacked into its own newly created, empty `*-unpacked` directory there. ZIP entries were checked for containment, backslash paths, and symlinks before extraction. Artifacts were treated as untrusted data: **no installer or processor was executed**. Each installer's downloaded official `.sha1` matched its local SHA-1. SHA-256 identifiers appear below to identify the exact samples; a repository-provided SHA-1 is an integrity check, not an independent signature.

Representative PowerShell commands, using a working curl and the same trust configuration:

```powershell
# $scratch is a fresh directory beneath the system temp directory.
# $curl is the selected curl.exe; $ca is the Windows-trust PEM bundle.
& $curl --noproxy '*' --cacert $ca -fLsS --max-time 60 `
  -w 'HTTP %{http_code}; bytes=%{size_download}' `
  'https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.10/profile/json' `
  -o (Join-Path $scratch 'fabric-profile.json')
& $curl --noproxy '*' --cacert $ca -fLsS --max-time 60 `
  'https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar' `
  -o (Join-Path $scratch 'forge-installer.jar')
```

`--noproxy '*'` was specific to the broken local proxy configuration, not a Nexora networking requirement. Curl response bodies were saved only in temp. Excerpts below are small selections from actual responses or unpacked artifacts; omitted fields do not imply an empty object or complete schema. This study read official loader sources, not GPL launcher implementations, and copies no launcher code. Official installer sources include LGPL notices: use them as behavioral references, not code to paste into an Apache-2.0 implementation.

**UNVERIFIED** means an execution claim, historical range, or future format was not demonstrated by these checks. Static inspection verifies metadata and advertised CLI support; it does not prove a successful install or game launch.

## 1. Fabric

### Official v2 endpoints

Base: `https://meta.fabricmc.net`. The following returned HTTP 200:

| Endpoint | Purpose | Observed response bytes |
| --- | --- | ---: |
| `/v2/versions/game` | Game versions and release/snapshot stability | 30,358 |
| `/v2/versions/loader` | Global loader list | 38,900 |
| `/v2/versions/loader/1.20.1` | Loader choices paired with mappings for this game | 590,278 |
| `/v2/versions/loader/1.20.1/0.16.10` | Selected pair and launcher metadata | 4,139 |
| `/v2/versions/loader/1.20.1/0.16.10/profile/json` | Client version profile | 2,859 |
| `/v2/versions/intermediary/1.20.1` | Intermediary mapping artifact | 104 |

Real intermediary response:

```json
[{"maven":"net.fabricmc:intermediary:1.20.1","version":"1.20.1","stable":true}]
```

Selected fields from the fetched profile:

```json
{
  "id": "fabric-loader-0.16.10-1.20.1",
  "inheritsFrom": "1.20.1",
  "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"
}
```

Its library list includes `net.fabricmc:intermediary:1.20.1` and `net.fabricmc:fabric-loader:0.16.10`, both using `https://maven.fabricmc.net/`. Some other libraries carry sizes and multiple hashes; do not assume every entry has those fields. URL-encode version path segments, particularly snapshot names and `+` characters. Pin the selected game/loader pair rather than resolving “latest” again during installation.

### Inheritance and intermediary

The profile is a child of vanilla, not a complete replacement. Resolve `inheritsFrom` against Mojang's official metadata, retain vanilla client download, assets, native libraries, logging and applicable Java requirements, then apply the child's entry point, libraries and arguments. Keep the unmodified vanilla client JAR available. Fabric's mapping JAR is a runtime dependency used for its intermediary namespace; Yarn is a development naming layer and is not an additional launcher requirement for this profile. Fabric API is a separate mod, not part of installing Fabric Loader.

Recommended Nexora merge policy: inherit absent scalar fields, let explicit child scalars override, combine applicable parent and child game/JVM arguments, and resolve library conflicts deliberately using group/artifact/classifier/extension identity. Preserve ordering and OS/architecture/feature rules; avoid putting two conflicting versions of one library on the classpath. Detect inheritance cycles. This is a proposed resolver policy, not a claim that arbitrary JSON deep merging is a standardized Minecraft operation. Do not lose vanilla authentication arguments because this child's `arguments.game` is empty.

No Forge-style patching processors are needed for the inspected Fabric profile. Intermediary must match the selected Minecraft version; copying another version's mappings into the library tree is not a compatibility strategy.

Sources: [Fabric Meta API specification](https://github.com/FabricMC/fabric-meta/blob/master/README.md), [live selected pair](https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.10), [live client profile](https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.10/profile/json), [live intermediary](https://meta.fabricmc.net/v2/versions/intermediary/1.20.1).

## 2. Quilt

### Official v3 equivalents

Base: `https://meta.quiltmc.org`. These checks returned HTTP 200:

| Endpoint | Purpose | Observed response bytes |
| --- | --- | ---: |
| `/v3/versions/game` | Supported game metadata | 24,814 |
| `/v3/versions/loader` | Global loader list | 144,120 |
| `/v3/versions/loader/1.20.1` | Loader/game combinations | 892,353 |
| `/v3/versions/loader/1.20.1/0.28.1` | Selected loader and mapping metadata | 2,553 |
| `/v3/versions/loader/1.20.1/0.28.1/profile/json` | Client profile | 1,512 |
| `/v3/versions/hashed/1.20.1` | Hashed mapping metadata | Checked; HTTP 200 |

Real selected-pair excerpt:

```json
{
  "loader": {
    "maven": "org.quiltmc:quilt-loader:0.28.1",
    "version": "0.28.1",
    "file_size": 2507287
  }
}
```

Real profile fields:

```json
{
  "id": "quilt-loader-0.28.1-1.20.1",
  "inheritsFrom": "1.20.1",
  "mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient"
}
```

The live pair additionally contains `hashed`, `intermediary`, `launcherMeta`, and nested `hashes`. The profile includes **both** `org.quiltmc:hashed:1.20.1` and `net.fabricmc:intermediary:1.20.1`. Quilt libraries use `https://maven.quiltmc.org/repository/release/`; several shared libraries use Fabric's Maven. Follow each entry's URL rather than replacing every library's repository with Quilt's.

### Differences from Fabric

Quilt's v3 model adds Hashed Mojmap and Quilt Mappings endpoints; it is not just Fabric v2 under another hostname. Loader metadata in the inspected response has `file_size` and a `hashes` object, and no Fabric-style loader `stable` field. Do not interpret absent stability as stable or unstable without a documented product policy. The fetched game-specific list began with `0.20.0-beta.9`, despite the separately fetched `0.28.1` profile being valid: **do not select element zero as the newest stable Quilt Loader**. Handle prereleases explicitly and validate the chosen pair.

Use the same vanilla inheritance machinery as Fabric, with Quilt's entry point and the complete returned library list. No install-time processors appear in the inspected profile. Quilt Standard Libraries/QFAPI are separate mod dependencies. Metadata availability alone does not establish that every mod or every advertised combination works.

The old `QuiltMC/quilt-meta` repository is archived and points to `QuiltMC/update-quilt-meta`; the current project's README points to the live API index. Treat the archived README as historical route documentation and the live responses as current schema evidence.

Sources: [live API index](https://meta.quiltmc.org/), [current metadata project](https://github.com/QuiltMC/update-quilt-meta), [archived v3 route documentation](https://github.com/QuiltMC/quilt-meta/blob/v3/README.md), [selected pair](https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.28.1), [profile](https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.28.1/profile/json).

## 3. Forge

### Discovery and artifact URLs

Use [official Maven metadata](https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml) for available `net.minecraftforge:forge` versions. HTTP 200; 213,223 bytes. A real excerpt is:

```xml
<groupId>net.minecraftforge</groupId>
<artifactId>forge</artifactId>
```

Use [official promotions](https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json) for per-Minecraft recommended/latest labels. HTTP 200; 4,113 bytes. Real selected entries:

```json
{
  "1.20.1-recommended": "47.4.10",
  "1.20.1-latest": "47.4.28"
}
```

The equivalent `promotions_slim.json` path on **maven.minecraftforge.net returned HTTP 404**. Keep its canonical `files.minecraftforge.net` host. Promotion values omit the Minecraft prefix: the matching full Maven version is `1.20.1-47.4.10`. Validate that full value against metadata. Historical versions may have extra suffixes, so retain exact repository version strings rather than regenerating them from a simplistic split.

Artifact pattern:

```text
https://maven.minecraftforge.net/net/minecraftforge/forge/{fullVersion}/forge-{fullVersion}-installer.jar
```

Do not treat Maven's global `<latest>` as “latest Forge for the selected Minecraft.” In this response it was `1.20.4-49.2.13`, while `<versions>` also included `1.21-51.0.33`. Parse and filter the version list and use the per-game promotions as labels, not a complete artifact index. The inspected builds below are reproducible historical samples, not recommendations to install those exact builds today.

### Three profile shapes, not “old = spec 0”

| Inspected build | `install_profile.json` shape | Game-launch metadata |
| --- | --- | --- |
| `1.12.2-14.23.5.2847` | Nested `install`, `versionInfo`, `optionals`; no `spec` | Embedded `versionInfo` |
| `1.12.2-14.23.5.2859` | Explicit `spec: 0`, top-level fields; empty processors | `/version.json` |
| `1.13.2-25.0.223` | Explicit `spec: 0`; five processors | `/version.json` |
| `1.16.5-36.2.39` | Explicit `spec: 0`; four processors | `/version.json` |
| `1.17.1-37.1.1` | `spec: 1`; extended tokens and server path | `/version.json` |
| `1.20.1-47.3.0` | `spec: 1`; side-filtered pipeline | `/version.json` |

Actual legacy fields from build 2847:

```json
{
  "path": "net.minecraftforge:forge:1.12.2-14.23.5.2847",
  "filePath": "forge-1.12.2-14.23.5.2847-universal.jar",
  "minecraft": "1.12.2"
}
```

These are fields within `install`, not the whole profile. The archive really contains the root-level universal JAR named by `filePath`. Its `versionInfo` inherits vanilla 1.12.2 and launches through LaunchWrapper with `FMLTweaker`. Install that payload at the Maven location of `install.path`, even though its archive filename has a `-universal` suffix and the coordinate has no classifier.

Explicit **spec 0 already supports data and processors**. The official current parser converts it into the newer internal model. Spec 1 adds installation-root/installer/library/version token capabilities, inline substitution and configurable server-JAR paths. Do not call all legacy nested profiles “spec 0” merely because no `spec` field exists. Detect structure before dispatching to a versioned parser. Likewise, **“spec 1+” is not a promise that all future specs are compatible**: the inspected Forge parser accepts 0 and 1 and rejects others. Unknown specs should produce an unsupported-format result.

`install_profile.json` describes installation. The file selected by its `json` field, normally `/version.json`, describes launching: `id`, `inheritsFrom`, entry point, arguments and runtime libraries. Resolve that child against the official vanilla version. Installation libraries and runtime libraries are different sets; fetch their union as needed, but do not add every install tool to the game's classpath.

Sources: [legacy installer 2847](https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2847/forge-1.12.2-14.23.5.2847-installer.jar), [repackaged 1.12.2 installer 2859](https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2859/forge-1.12.2-14.23.5.2859-installer.jar), [1.13.2 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.13.2-25.0.223/forge-1.13.2-25.0.223-installer.jar), [official profile parser](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/json/Util.java), [spec 1 extensions](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/json/InstallV1.java).

### Data resolution and extraction

Select `client` values from every relevant `data` entry; do not substitute server data into a client install. For example, the actual 1.16.5 profile contains:

```json
{
  "BINPATCH": {"client":"/data/client.lzma","server":"/data/server.lzma"},
  "PATCHED": {
    "client":"[net.minecraftforge:forge:1.16.5-36.2.39:client]",
    "server":"[net.minecraftforge:forge:1.16.5-36.2.39:server]"
  },
  "PATCHED_SHA": {
    "client":"'07678ff3edd6990a75bf0707ad2dbb2315b8000f'",
    "server":"'9a76786a85d28231a3c171387039b99200a086ba'"
  }
}
```

Interpret the forms distinctly:

| Form | Meaning |
| --- | --- |
| `[group:artifact:version:classifier@extension]` | Absolute local path beneath the installation's Maven-style `libraries` tree; classifier is optional and extension defaults to JAR |
| `'literal text'` | Literal string after removing the surrounding single quotes; hashes use this form |
| `/data/client.lzma` or other unquoted resource name | An entry inside the installer, extracted to controlled temp storage; use its absolute extracted path |
| `{TOKEN}` in processor arguments/output metadata | Substitute the resolved data or an installer-provided built-in value |

For example, `net.minecraft:client:1.20.1-20230612.114412:mappings@txt` maps to `net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-mappings.txt` under `libraries`. Square brackets designate a path, **not a command to download that artifact**. Many such paths are outputs created by earlier processors and intentionally have no public download URL.

Built-ins include `SIDE` and `MINECRAFT_JAR`; the extended model also supplies `ROOT`, `INSTALLER`, `LIBRARY_DIR` and `MINECRAFT_VERSION`. Keep install-time `{TOKEN}` expansion separate from launch-time `${token}` expansion. Recognize archive-leading `/` as resource syntax, never as permission to write to a filesystem root. Reject traversal or unexpected output paths after expansion.

Required extraction is metadata-driven:

1. Read `install_profile.json` and its selected version JSON, or legacy `versionInfo`.
2. Extract the legacy `install.filePath` payload for nested profiles.
3. For modern profiles, install required embedded `maven/<artifact-path>` entries into the matching `libraries/<artifact-path>` locations. An empty download URL can indicate locally supplied/generated content; check embedded resources and processor outputs before declaring a library unavailable.
4. Extract each selected side's installer-resource data value, especially `data/client.lzma` for the inspected patching pipelines.
5. Preserve access to the original installer for processors using `{INSTALLER}`. Inspect processor arguments for other resource extractions. The inspected `run.bat`, `run.sh`, `user_jvm_args.txt`, `win_args.txt`, and `unix_args.txt` extraction task is **server-only**; those files are not a universal client prerequisite.
6. GUI logos/icons are presentation resources, not game libraries. Optional offline installers may contain cached outputs; do not assume every installer contains them.

In 1.16.5 the archive includes both a universal JAR and an unclassified Forge JAR under `maven/`. The sampled 1.17.1 and 1.20.1 archives have patch data but no `maven/` payload tree. Do not hard-code “always extract universal.jar.”

`BINPATCH` names the compressed patch payload, not a launchable JAR. In the sampled 1.16.5 pipeline, tools obtain mappings, split vanilla into slim/extra artifacts, remap the slim classes, then apply `client.lzma` through BinaryPatcher. Preserve the declared steps and their inputs: applying the patch directly to an arbitrary vanilla or already-patched JAR is incorrect.

Sources: [1.16.5 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-installer.jar), [1.17.1 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.17.1-37.1.1/forge-1.17.1-37.1.1-installer.jar), [1.20.1 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar), [official resource extraction behavior](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/DownloadUtils.java).

### Processors: execution and validation

Each processor has an executable Maven coordinate in `jar`, dependency coordinates in `classpath`, an ordered `args` array, optionally a `sides` filter, and optionally an `outputs` map. Omitted `sides` means applicable to both client and server; an explicit server-only task must be skipped for a client install. Run applicable processors **sequentially in declared order**.

Real 1.16.5 processor excerpt, with `classpath` omitted here for brevity:

```json
{
  "jar": "net.minecraftforge:binarypatcher:1.0.12",
  "args": ["--clean","{MC_SRG}","--output","{PATCHED}","--apply","{BINPATCH}"],
  "outputs": {"{PATCHED}":"{PATCHED_SHA}"}
}
```

Resolve output keys into filesystem paths and values into expected SHA-1 strings. A processor with declared outputs can be skipped only when **all** declared outputs exist and their hashes match. Validate again after execution, and fail installation on missing or mismatched outputs. An absent/empty output map provides no hash-based skip condition. Some inspected newer profiles do not declare hashes for their final patched artifact; do not invent a checksum from a different release.

For Nexora, spawn the chosen `java.exe` directly with argument-vector handling. Read the processor JAR's `META-INF/MANIFEST.MF` and use its `Main-Class`; place that JAR and all declared `classpath` dependencies on the Java classpath. Conceptually:

```text
<java> -cp <processor.jar + OS-separated dependency paths> <manifest Main-Class> <resolved args...>
```

Do not rely on `java -jar` to honor an independently supplied `-cp`. The official installer instead invokes processor entry points inside its own JVM using an isolated classloader; separate Java processes are Nexora's proposed adaptation. Preserve classpath ordering, the full argument array, exit status, logs and cancellation. Do not derive the processor entry point from the game profile's `mainClass`.

A separate real [BinaryPatcher 1.1.1 JAR](https://maven.minecraftforge.net/net/minecraftforge/binarypatcher/1.1.1/binarypatcher-1.1.1.jar) was downloaded with curl and its manifest read:

```text
Implementation-Version: 1.1.1
Main-Class: net.minecraftforge.binarypatcher.ConsoleTool
```

Choose processor Java by the toolchain's actual compatibility requirements, separately from the final game's Java. A Java-8-compatible installer does not imply that every processor can handle modern class files on Java 8, nor that old tooling works on the newest JVM. Start testing with the game's required Java, and retain a per-toolchain override. **UNVERIFIED:** successful execution of these pipelines, exact minimum JVM per historical processor release, and subprocess equivalence for every tool. No processors ran during this research.

Sources: inspected [1.16.5 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.39/forge-1.16.5-36.2.39-installer.jar), [1.13.2 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.13.2-25.0.223/forge-1.13.2-25.0.223-installer.jar), [1.20.1 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar), [official processor runner](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/actions/PostProcessors.java), [processor field definitions](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/json/Install.java).

### Behavior by Minecraft era and minimum support

| Minecraft era | Observed behavior / required distinction |
| --- | --- |
| `<=1.12.2` | Sampled 1.12.2 uses LaunchWrapper and FMLTweaker, vanilla inheritance and a supplied Forge library. Build 2847 has a nested legacy profile/universal payload; 2859 is repackaged spec 0 with no processors. **UNVERIFIED:** a single rule covering every earlier MC release, ancient JAR-mod installation, and historical packed library variants. |
| `1.13–1.16` | Sampled 1.13.2 and 1.16.5 use spec 0 processors and separate version JSON. 1.16.5 launches via `cpw.mods.modlauncher.Launcher`; patching is install-time work. Exact earliest 1.13 build behavior is **UNVERIFIED**. |
| `1.17+` | Sampled 1.17.1 uses spec 1, `cpw.mods.bootstraplauncher.BootstrapLauncher`, module-path arguments, module access flags, mapping download/merge and binary patching. Keep module-path arguments separate from ordinary classpath construction. |
| `1.20.x` | Sampled 1.20.1 retains spec 1/BootstrapLauncher, uses ForgeAutoRenamingTool, and has server bundler/extraction tasks that client installs skip. Use each actual profile's module path, ignore/merge properties and FML arguments. Java changes within 1.20.x; see below. |
| `1.21 and later` | Do not extrapolate compatibility from a version number. Native Forge installer behavior beyond sampled 1.20.1 is **UNVERIFIED** here; inspect and qualify selected builds before advertising support. NeoForge 1.21.1 was inspected separately. |

Real 1.17.1 launch fields include `"mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher"`; its JVM list includes `-p`, `${classpath_separator}`, `--add-modules`, `ALL-MODULE-PATH` and module access flags. Substitute `${library_directory}` and `${classpath_separator}` using the target OS, retain values containing commas as single arguments, and honor rule objects. Do not hard-code a BootstrapLauncher classpath as the complete launch command.

**Recommended minimum: Minecraft 1.13.2, with qualified Forge builds.** It is the earliest sampled release with the common processor/profile architecture and avoids implementing the separate nested legacy family, LaunchWrapper-specific argument handling and older library quirks in the first release. This is an engineering scope recommendation, not a claim of continuing upstream support. If 1.12.2 modpacks become a requirement, add a separately tested legacy adapter; simply lowering a numeric minimum is insufficient. A smaller initial validation matrix could start at 1.16.5, while retaining the same parser design.

Sources: the six Forge installer artifacts linked above; [1.17.1 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.17.1-37.1.1/forge-1.17.1-37.1.1-installer.jar) and [1.20.1 installer](https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar) provide the module-path evidence.

## 4. NeoForge

### Official Maven endpoints and numbering

Both metadata endpoints returned HTTP 200:

| URL | Role | Bytes |
| --- | --- | ---: |
| `https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml` | Modern NeoForge coordinates | 67,959 |
| `https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml` | Minecraft 1.20.1 Forge-named NeoForge line | 2,830 |

Actual artifact identity from the second response:

```xml
<groupId>net.neoforged</groupId>
<artifactId>forge</artifactId>
```

The sampled mappings are:

| Minecraft | Exact coordinate | Installer path relative to `/releases/` |
| --- | --- | --- |
| `1.20.1` | `net.neoforged:forge:1.20.1-47.1.106` | `net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar` |
| `1.20.4` | `net.neoforged:neoforge:20.4.237` | `net/neoforged/neoforge/20.4.237/neoforge-20.4.237-installer.jar` |
| `1.21.1` | `net.neoforged:neoforge:21.1.172` | `net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar` |

For the 1.20/1.21-era scheme, `20.4` corresponds to Minecraft 1.20.4 and `21.1` to 1.21.1; the trailing component is the NeoForge build, with prerelease suffixes where applicable. **Do not universally prepend `1.` to NeoForge versions**: the live modern metadata now also contains `26.1.0.0-alpha.1+snapshot-1` and reports `<latest>26.1.2.121</latest>`. Mapping and installer behavior for that later numbering family are **UNVERIFIED** here. Preserve exact versions and validate the install profile's `minecraft` and version JSON's `inheritsFrom` before selecting the vanilla base.

### Installer differences

All three sampled NeoForge installers use `spec: 1`, `/version.json`, client/server data, Java processors and BootstrapLauncher profiles. They fit the same general installation engine, with coordinates, tool versions, data and launch arguments supplied by the selected artifact.

Do not globally replace `minecraftforge` strings with `neoforged`. The 1.20.1 sample contains an embedded `maven/net/neoforged/forge/...-universal.jar`, but still invokes `net.minecraftforge:installertools` and uses Forge-style FML arguments. The 20.4/21.1 samples use NeoForm mapping coordinates, `net.neoforged.installertools:installertools:2.1.2`, and arguments such as `--fml.neoForgeVersion`, `--fml.fmlVersion` and `--fml.neoFormVersion`. Their launch target remains `forgeclient`; infer nothing from that label alone.

Real 21.1.172 profile selection:

```json
{
  "id":"neoforge-21.1.172",
  "inheritsFrom":"1.21.1",
  "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher"
}
```

The 20.4.237 and 21.1.172 profiles contain **no processor `outputs` maps**, unlike several sampled Forge pipelines. Consequently, absence of hashes is a real supported input shape, not a parsing failure. Require successful processing and required launch artifacts; do not silently invent Forge hashes or use mere file existence as a universal cache proof.

The inspected installer manifests still name `net.minecraftforge.installer.SimpleInstaller`, despite the NeoForge branding. This Java package is not a reliable loader identifier. Modern source is in `neoforged/LegacyInstaller`, not the guessed `neoforged/Installer` repository. NeoForge's installer offers some additional options in current source; use only capabilities verified for the selected installer build.

Sources: [NeoForge metadata](https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml), [Forge-named metadata](https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml), [1.20.1 installer](https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar), [20.4.237 installer](https://maven.neoforged.net/releases/net/neoforged/neoforge/20.4.237/neoforge-20.4.237-installer.jar), [21.1.172 installer](https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar), [official installer source](https://github.com/neoforged/LegacyInstaller), [official InstallerTools compatibility note](https://github.com/neoforged/InstallerTools).

## 5. Alternative: invoke the official client installer

**Yes: modern Forge and NeoForge installers explicitly support `--installClient`.** Official entry-point source registers it as a client-install action with an optional directory argument and selects non-GUI operation for that action. Current NeoForge source also advertises `--install-client`. The literal `installClient` is present in the downloaded Forge 47.3.0 and NeoForge 20.4.237/21.1.172 entry-point class files. This is explicit CLI functionality, not GUI automation or a guessed flag.

Use an explicit target and working directory:

```text
<java.exe> -Djava.awt.headless=true -jar <absolute-installer.jar> --installClient <absolute-staging-root>
```

`-Djava.awt.headless=true` is a JVM property, not the action selector. `--installServer` installs different artifacts and is not a substitute for client installation. Check a selected installer's own `--help` in a controlled environment before depending on version-specific options. The old 1.12.2 build 2847 entry-point class did **not** contain `installClient`; a uniform legacy CLI contract is **UNVERIFIED**.

The inspected Forge/NeoForge client-install sources expect a Minecraft-style directory with `launcher_profiles.json` or `launcher_profiles_microsoft_store.json`. They install a version under `versions`, populate `libraries`, obtain vanilla inputs and update launcher profile data. A standalone Nexora instance therefore needs an isolated, compatible staging layout, including a valid minimal launcher profile document for builds requiring it. They do not replace Nexora's complete vanilla/assets/natives installation or authentication workflow.

| Approach | Advantages | Costs / limitations |
| --- | --- | --- |
| Official installer subprocess | Upstream owns processor orchestration and embedded-resource handling; fewer format details to reproduce; useful fallback and behavior oracle | Requires Java and launcher-compatible layout; writes profiles/logs and may make its own network/mirror choices; coarse progress/cancellation; output discovery and integrity checks still needed |
| Nexora resolves metadata and runs declared tools | Controlled official-source downloads, cache policy, progress, retries, cancellation and instance layout; reusable Forge/NeoForge engine | Must implement parsing, side/data resolution, extraction, ordering and output checks correctly; needs release qualification when upstream changes |

Suggested fallback procedure: prepare vanilla and launcher-compatible staging, run with an explicit Java/target/cwd, capture logs, check exit status **and resulting profile/artifacts**, then import the validated installation atomically. Do not run against the user's existing official-launcher directory by default. **UNVERIFIED:** an actual headless client install, acceptance of a particular minimal profile document by every installer, and artifact-for-artifact equivalence with a Nexora runner. These were not executed in this research.

Sources: [Forge CLI entry point](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/SimpleInstaller.java), [Forge client action](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/actions/ClientInstall.java), [NeoForge CLI entry point](https://github.com/neoforged/LegacyInstaller/blob/main/src/main/java/net/minecraftforge/installer/SimpleInstaller.java), [NeoForge client action](https://github.com/neoforged/LegacyInstaller/blob/main/src/main/java/net/minecraftforge/installer/actions/ClientInstall.java), and the exact installer artifacts in sections 3–4.

## 6. Known pitfalls and operational decisions

### Hash mismatches and incomplete caches

Treat a processor output hash mismatch as installation failure, even when Java exits successfully. Log processor coordinate, expected/actual hash, input identities and Java version. Retry from verified inputs and remove only failed outputs within the controlled installation root. Do not globally disable checks to make installation appear successful. Possible causes include stale generated artifacts, wrong vanilla inputs, mixed Minecraft/loader versions, corruption or tool/JVM differences; **the exact cause requires diagnosis**, not a blanket conclusion from the mismatch alone.

Some processors have no output hashes, and some intermediate hashes exist while the final patch has none. Cache validity must reflect the actual declarations. Use an install record keyed by installer hash, vanilla hash, selected side and relevant tool/runtime inputs, then validate required final files. This record is a proposed Nexora mechanism, not an upstream standard. Parallelize independent downloads, not sequentially dependent processors.

Sources: [Forge processor output validation](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/actions/PostProcessors.java), [NeoForge processor runner](https://github.com/neoforged/LegacyInstaller/blob/main/src/main/java/net/minecraftforge/installer/actions/PostProcessors.java), and the inspected 1.16.5/20.4.237/21.1.172 profiles.

### Java requirements

Mojang's official per-version JSON was fetched with curl. Real `javaVersion.majorVersion` values:

| Minecraft sample | Declared Java major |
| --- | ---: |
| `1.16.5` | 8 |
| `1.17.1` | 16 |
| `1.18.2` | 17 |
| `1.20.1`, `1.20.4` | 17 |
| `1.20.6`, `1.21.1` | 21 |

For example, 1.20.6 returns `"javaVersion":{"component":"java-runtime-delta","majorVersion":21}`. The Java 21 transition is **1.20.5**, not “all 1.20.x”; NeoForged's release announcement confirms it. Later releases must be resolved from their own metadata rather than assuming Java 21 forever.

These are **game runtime requirements**, not a verified minimum for each processor. The official NeoForge InstallerTools project describes Java 8 compatibility for its tools, while patched game inputs and other tools can impose additional constraints. Record the separately selected installation JVM and game JVM; do not simply execute whatever `java` happens to be on PATH. **UNVERIFIED:** historical processor/JVM compatibility matrix beyond static requirements; qualify actual builds on Java 8/16/17/21 as applicable.

Sources: [Mojang version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json), [1.16.5 JSON](https://piston-meta.mojang.com/v1/packages/fba9f7833e858a1257d810d21a3a9e3c967f9077/1.16.5.json), [1.17.1 JSON](https://piston-meta.mojang.com/v1/packages/e0e7ab5ed6f55bbd874ef95be3c9356d67e64b57/1.17.1.json), [1.18.2 JSON](https://piston-meta.mojang.com/v1/packages/334b33fcba3c9be4b7514624c965256535bd7eba/1.18.2.json), [1.20.1 JSON](https://piston-meta.mojang.com/v1/packages/a5cf661d31d9dfcd3d1645be80f43954f63f63b0/1.20.1.json), [1.20.4 JSON](https://piston-meta.mojang.com/v1/packages/2c592ccd71ca2d5dadebeb6543ad6b18e6f43305/1.20.4.json), [1.20.6 JSON](https://piston-meta.mojang.com/v1/packages/876ef010b51e9e646712d8322e383a84859d3b39/1.20.6.json), [1.21.1 JSON](https://piston-meta.mojang.com/v1/packages/499680d659b7a0ccb2fa9a809c345c5baf2ace7d/1.21.1.json), [NeoForge 20.5 announcement](https://neoforged.net/news/20.5release/).

### Windows paths and process handling

Use process argument vectors, not a `cmd.exe` command assembled from metadata. Windows Java classpath and module-path lists use `;`, while Unix uses `:`; do not split Maven coordinates on the filesystem separator. Preserve Unicode and spaces and use the platform's subprocess quoting rules. Avoid stripping the significant spaces in a property value merely because it looks unusual. Account for command-line length; use Java argument files only with JVM versions that support them and correct argument-file escaping.

The official installer explicitly rejects being run from a path containing `!/`, a Java archive URL limitation. Avoid trailing `!` directory names for staging. Distinguish archive `/data/...` resources from Windows rooted paths; normalize and verify extraction/output containment, including drive/UNC paths, traversal, symlinks and case collisions. Close JAR handles before replacement and consider antivirus/file-lock failures. Cancel entire Java process trees and leave the instance uncommitted until validation finishes. These are proposed hardening requirements; Windows installs with spaces, Unicode and long paths are **UNVERIFIED by execution here**.

Sources: [Forge installer path check](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/SimpleInstaller.java), inspected module-path arguments in [Forge 1.20.1](https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar) and [NeoForge 21.1.172](https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-installer.jar).

### Slow repositories, mirrors, and metadata changes

Prefer the loader's official metadata and the artifact URLs declared there; this design needs no Prism/Modrinth metadata mirror. An official profile can legitimately reference several repositories. Bound concurrency per host, retry transient failures with backoff, distinguish 404 from timeout/429/5xx, use partial-file staging and validate size/hash before promotion into cache. Cache immutable artifacts by identity and refresh mutable discovery metadata with HTTP validators when available. Keep a last-known valid metadata cache and expose stale/offline state clearly.

Forge profiles may declare a mirror list, and the official installer may choose a mirror independently. An installer subprocess therefore offers less control over a strict direct-source policy; inspect or constrain its supported network options rather than claiming the top-level download URL controls every subsequent download. Do not silently fall back to a third-party metadata mirror. **UNVERIFIED:** a persistent current “slow mirror” problem or representative throughput comparison. This session's successful sampled downloads are not a performance benchmark.

Sources: [Forge profile mirror selection](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/json/Install.java), [Forge CLI mirror option](https://github.com/MinecraftForge/Installer/blob/2.0/src/main/java/net/minecraftforge/installer/SimpleInstaller.java), official profile URLs in sections 1–4.

## 7. Inspected installer identifiers

All nine installer requests and their `.sha1` sidecars returned HTTP 200. Every SHA-1 matched. Artifact URLs are linked in sections 3–4; SHA-256 below identifies downloaded bytes, not an upstream signature.

| Loader and full version | Bytes | SHA-256 |
| --- | ---: | --- |
| Forge `1.12.2-14.23.5.2847` | 5,336,478 | `3a74473fc62dcf13baa4130e6ea31a80c6a872b6b25f1a4c9195c8e878415bd0` |
| Forge `1.12.2-14.23.5.2859` | 4,681,890 | `dbba1cd1bf34a4b82148bac50a6150f63ffcb2d5dc7d88a0508c27fdcff15b76` |
| Forge `1.13.2-25.0.223` | 6,152,532 | `d46cb326e7dc32695084090c8169bb57f32f1c23b801ddb35a336f252e381191` |
| Forge `1.16.5-36.2.39` | 7,481,495 | `bc23bd0e0c6f17a7dfdddbea65f008c8126ad8f9bb84b9a47069a2ba8d02fccd` |
| Forge `1.17.1-37.1.1` | 5,364,773 | `31f2319b1b9ca491aaccc020887240f8524a4ff2a8e0f8b795e17e889cbc043b` |
| Forge `1.20.1-47.3.0` | 6,021,040 | `6018abce95cc058768e365865fd7cf3bd31b5c553231daf885dc385fcd6d875a` |
| NeoForge `1.20.1-47.1.106` | 7,846,890 | `c1ea1c3be532c4444004efd7f9cc71e1590d010ff44845b93effba61e3bd1526` |
| NeoForge `20.4.237` | 6,456,359 | `8b7de340c33da5f69279906cae53d8083329a8f6fe426efec57b8560a322b8d0` |
| NeoForge `21.1.172` | 6,922,832 | `3fd19389ffc21d528ce5411a7e650d09201c9d7c911578362a17fa94559eb894` |

## Implementation checklist

- [ ] Fetch vanilla manifests/profiles from Mojang; implement inheritance, launch rules, natives, assets, authentication placeholders and Java selection.
- [ ] Add official Fabric v2 and Quilt v3 discovery adapters; URL-encode segments, preserve exact versions and handle missing stability fields/prereleases.
- [ ] Consume selected client profiles and all declared repositories/mappings; keep development mappings and API mods separate.
- [ ] Discover Forge versions from official Maven XML and label per-game promotions from `files.minecraftforge.net`.
- [ ] Discover both NeoForge coordinate families; confirm Minecraft identity from installer/profile metadata rather than a permanent numbering heuristic.
- [ ] Download installers and sidecars, record provenance/hashes, and inspect archives in bounded isolated temp storage.
- [ ] Detect nested legacy profiles separately from explicit spec 0/1. Reject unknown formats/specs without silently guessing.
- [ ] Resolve client-side data, Maven coordinates, quoted literals, installer resources and supported built-ins. Keep install/launch substitutions distinct.
- [ ] Install embedded Maven payloads and extract selected resources; retain the installer for processor references.
- [ ] Fetch installation tools separately from runtime libraries; recognize generated artifacts instead of requesting nonexistent public downloads.
- [ ] Resolve `Main-Class` from each processor manifest; execute an explicit compatible Java with ordered classpath/args, logs, exit checks and cancellation.
- [ ] Filter sides, preserve processor ordering, validate declared output SHA-1 values and avoid unsafe cache skips when hashes are absent.
- [ ] Preserve BootstrapLauncher module-path/access arguments and OS-specific separators; support legacy argument strings only in an explicit legacy adapter.
- [ ] Implement atomic instance completion, bounded retries, per-host concurrency and actionable failure reports.
- [ ] If adding the official-installer fallback, qualify `--installClient`, a staged launcher-compatible profile document, generated outputs and network behavior per installer generation.
- [ ] Qualify clean install, reinstall/cache recovery, corrupted outputs, cancellation, offline failures, spaces/Unicode/long Windows paths and required Java versions on representative builds. These execution checks remain **UNVERIFIED** in this documentation-only study.
- [ ] Keep source/license provenance. Reimplement from formats and observed behavior; do not import GPL launcher code into Nexora.

## Loader × Minecraft version range × install method

These are proposed methods, not a claim that Nexora currently implements or has executed them. Range rows summarize sampled families; exceptions require inspecting the actual installer.

| Loader | Minecraft version range | Install method | Evidence / support decision |
| --- | --- | --- | --- |
| Fabric | API-advertised supported versions | Official v2 client profile + vanilla inheritance + declared Maven libraries | Profile verified for 1.20.1/0.16.10; validate each chosen pair |
| Quilt | API-advertised supported versions | Official v3 client profile + vanilla inheritance + Quilt hashed/intermediary dependencies as returned | Profile verified for 1.20.1/0.28.1; do not assume first list entry is latest stable |
| Forge | `<=1.12.2` | Legacy nested-profile adapter + supplied universal/library payload + LaunchWrapper; detect modern repacks separately | 2847 and 2859 both inspected; earlier releases **UNVERIFIED**; defer from initial support |
| Forge | `1.13–1.16.x` | Spec 0 interpreter + ordered client processors + inherited version JSON | 1.13.2 and 1.16.5 inspected; recommend 1.13.2 minimum with qualified builds |
| Forge | `1.17–1.19.x` | Spec 1 interpreter + client processors + BootstrapLauncher/module-path launch | 1.17.1 inspected; exact intervening builds require qualification |
| Forge | `1.20–1.20.4` | Same engine; inspect mapping/remapping tools and side filters; Java 17 baseline | 1.20.1 installer and 1.20.4 vanilla Java verified |
| Forge | `1.20.5–1.21.x` | Inspect selected installer, then compatible processor engine; Java 21 game baseline | Java transition verified; native Forge installers in this range **UNVERIFIED** |
| NeoForge | `1.20.1` | `net.neoforged:forge` installer; spec 1 engine with Forge-era coordinates/arguments | 47.1.106 inspected; Java 17 game baseline |
| NeoForge | `1.20.2–1.20.4` | `net.neoforged:neoforge` installer; spec 1 engine + NeoForm/tools and returned module arguments | 20.4.237 inspected; individual 1.20.2/1.20.3 installers **UNVERIFIED** |
| NeoForge | `1.20.5–1.21.x` | Modern coordinates; inspect spec/data/processor graph and preserve returned launch arguments | 21.1.172 inspected; Java 21 game baseline; no output maps in this sample |
| Forge / NeoForge | Modern builds explicitly supporting the action | Official `--installClient <staging-root>` subprocess, then validate/import | CLI support statically verified; successful headless installation **UNVERIFIED** |
| Forge / NeoForge | Later/new numbering families, including `26.x` | Shape inspected statically on 2026-10-20 (see Phase 5-2a findings), then enabled | Forge 26.3-66.0.13 and NeoForge 26.3.0.95 installers match the spec 1 engine; execution **UNVERIFIED** |

## Phase 5-2a findings

Forge (Minecraft 1.16.5–1.21.x and 26.x) and NeoForge (1.20.1–1.21.x and 26.x) install support was implemented in phase 5-2a. The raw inspection data is in `crates/nexora-core/tests/fixtures/README.md` (forge section, fetched 2026-10-20). Installers were only read with `unzip`, never executed.

### Established by fixture inspection

- `version.json` has no `jar` field in any of the four inspected installers (Forge 1.16.5-36.2.39, Forge 1.20.1-47.3.0, NeoForge 21.1.172, `net.neoforged:forge` 1.20.1-47.1.106). The version merge sets `jar` itself, naming the version after the owner of the client jar.
- The presence of processor `outputs` varies: Forge 1.20.1 has them only on the side-filtered jarsplitter processors, NeoForge 21.1.172 has none, and `net.neoforged:forge` 1.20.1-47.1.106 has them like Forge 1.20.1. Output hashes are checked only where declared; a processor with no declared outputs is never skipped on a cache assumption.
- Empty artifact URLs occur only in Forge 1.16.5 (`net.minecraftforge:forge:1.16.5-36.2.39` and its `universal`). Those files come from the installer's `maven/` directory, so a library without a download URL is planned as a generated file instead of a download.
- Minecraft 26.x was first excluded and then enabled on 2026-10-20 after a static shape check of the Forge 26.3-66.0.13, Forge 26.2-65.1.0 and NeoForge 26.3.0.95 installers (read only, never executed; fixtures `forge-26.3-*` and `neoforge-26.3-*`). They use the shape the engine already handles: spec 1, `/version.json`, `inheritsFrom` = the 26.x vanilla id, no `jar` field. Forge 26.3 has 5 processors, of which only the `binarypatcher:1.3.4` one is client-side (the other four are server-only), and `net.minecraftforge:forge:26.3-66.0.13:client` is an empty-URL library provided from the installer's `maven/` (one generated file). NeoForge 26.3.0.95 has 2 processors (the `installertools:4.0.17:fatjar` one has no `sides` and runs on the client), no `maven/` payload, and main class `net.neoforged.fml.startup.Client`. Mojang 26.3 needs Java 25 (`java-runtime-epsilon`), which the existing runtime handling covers.
- Version numbers: Forge 26.x looks like `26.3-66.0.13` (Minecraft part before the first `-`). NeoForge 26.x looks like `26.3.0.95`: the Minecraft version is the first three numeric components with a trailing `.0` dropped (`26.1.0.x` is `26.1`, `26.1.2.x` is `26.1.2`); the rule for 20.x/21.x (`21.0.5` is `1.21`) is unchanged. Any `-`/`+` suffix (`beta`, `alpha`, `snapshot`) marks a non-stable version.
- Maven metadata order is neither chronological nor by version (NeoForge `21.1.263` appears after `26.3.0.92`), and `<latest>` points at `26.x` builds. Versions are sorted and filtered by the supported range instead.
- For NeoForge 1.20.1 the old family `net.neoforged:forge` is used; `net.neoforged:neoforge` starts at 1.20.2.
- The installer cache is keyed by the full URL, because the Forge and NeoForge 1.20.1 installers share the file name `forge-1.20.1-<version>-installer.jar`.
- `${version_name}` in JVM arguments is set to the vanilla version id. `${library_directory}` and `${classpath_separator}` are provided by the launcher. This substitution is shared with Fabric and Quilt, so their `--version` value changes as well.
- Processor Java is the game's Java runtime (Java 8 for 1.16.5, 17 for 1.20.1, 21 for 1.21.x).
- Launch uses only the pinned profile and the completion record (`nexora-loader.json`, which lists the files the install provided relative to `shared/`). It reports missing loader files and marks the instance broken so that the next Play reinstalls; it never fetches the installer.

### Still UNVERIFIED

- Running real processors with a real Java (all tests use a fake Java).
- Launching any Forge or NeoForge instance to the title screen on a real machine, including offline launch.
- Whether the game's Java is the right choice for processors, notably Java 8 for Forge 1.16.5.
- Whether the `${version_name}` change affects Forge startup.
- Builds other than those inspected. Forge 1.20.5 and later (up to the latest 1.21.x) is offered in the UI, but its real behaviour is UNVERIFIED.
- Minecraft 26.x for both loaders: verified statically against the 26.3 installers only (plus the Forge 26.2 installer for shape); running the processors and launching the game are UNVERIFIED.
- Legacy Forge (1.12.2 and earlier, nested profile format) is not supported.

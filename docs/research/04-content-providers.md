# Content providers: Modrinth first, CurseForge later

Research date: **2026-10-15**. Scope: a new Rust/Tauri 2 Minecraft Java launcher, with Apache-2.0 planned. Original documentation only; no Prism, MultiMC, or Modrinth App source was used or copied.

## Evidence and limitations

- **LIVE** means a response obtained during this research. **Documented** means checked against linked primary documentation, rather than integration-tested.
- **UNVERIFIED** marks facts or behavior not established here. Proposed Nexora policies are recommendations, not provider guarantees.
- **Successful HTTPS curl verification remains UNVERIFIED.** Real curl calls were executed for all principal Modrinth endpoints below, but both installed Windows curl builds use Schannel and failed before receiving HTTPS responses. Independent HTTPS probes succeeded with Python `requests`, normal certificate verification, and `trust_env=False`. Those results are not curl results.
- No CurseForge API key was available. Authenticated behavior and numeric catalog mappings remain **UNVERIFIED**. No pack archive or JAR was installed or executed.

### Curl evidence

Initial curl through the configured proxy failed with error 7 connecting through `127.0.0.1`. Direct requests with `--noproxy '*'` failed at TLS:

```text
curl: (35) schannel: AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS (0x8009030e)
```

Curl attempts to search, project, filtered version list, exact version, SHA-1 lookup, SHA-512 lookup, bulk lookup, and bulk update all exited 35. Disabling certificate verification did not resolve this local credential error. No HTTPS status/body/headers were obtained through curl.

An actual **HTTP curl** probe succeeded, demonstrating only the HTTPS redirect:

```sh
curl --noproxy '*' -sS -i --max-time 15 \
  -H 'User-Agent: NexoraLauncher/0.0.0 (content-provider-research)' \
  'http://api.modrinth.com/v2/project/sodium'
```

```text
HTTP/1.1 301 Moved Permanently
Date: Thu, 15 Oct 2026 07:01:23 GMT
Location: https://api.modrinth.com/v2/project/sodium
```

The following Bash-compatible commands reproduce the intended HTTPS checks on a machine with working curl TLS. They write no response files. Hash/version values came from the successful Python probes. The same endpoint paths and POST bodies were attempted with curl here; the failed curl search used `query=sodium&limit=1`, while the successful Python search included the full facets below.

```sh
base='https://api.modrinth.com/v2'
ua='NexoraLauncher/0.0.0 (content-provider-research)'
sha1='003c114c85ca88ef3362e018deb6aca0c682d6a1'
sha512='8614a374593698069a80ac74de8cd9c28f3928d16819a0e0d6320b538e823f7e54e8be06d5ea5a9c268beb0e02073209d1f831a02dede851588b972367680853'
curl -sS -i -H "User-Agent: $ua" --get "$base/search" \
  --data-urlencode 'query=sodium' --data-urlencode 'limit=1' \
  --data-urlencode 'facets=[["project_type:mod"],["categories:fabric"],["versions:1.21.1"],["client_side:required","client_side:optional"]]'
curl -sS -i -H "User-Agent: $ua" "$base/project/sodium"
curl -sS -i -H "User-Agent: $ua" --get "$base/project/sodium/version" \
  --data-urlencode 'loaders=["fabric"]' \
  --data-urlencode 'game_versions=["1.21.1"]' \
  --data-urlencode 'include_changelog=false'
curl -sS -i -H "User-Agent: $ua" "$base/version/SMxNOGZ6"
curl -sS -i -H "User-Agent: $ua" "$base/version_file/$sha1?algorithm=sha1"
curl -sS -i -H "User-Agent: $ua" "$base/version_file/$sha512?algorithm=sha512"
curl -sS -i -H "User-Agent: $ua" -H 'Content-Type: application/json' \
  --data-raw '{"hashes":["003c114c85ca88ef3362e018deb6aca0c682d6a1"],"algorithm":"sha1"}' \
  "$base/version_files"
curl -sS -i -H "User-Agent: $ua" -H 'Content-Type: application/json' \
  --data-raw '{"hashes":["003c114c85ca88ef3362e018deb6aca0c682d6a1"],"algorithm":"sha1","loaders":["fabric"],"game_versions":["1.21.1"]}' \
  "$base/version_files/update"
```

### Independent LIVE HTTPS results

At approximately 07:01 UTC, Python HTTPS requests returned:

| Request, relative to `/v2` | Status | Small response excerpt or projection |
| --- | --- | --- |
| `/project/sodium` | 200 | `{"id":"AANobbMI","slug":"sodium","project_type":"mod"}` |
| `/search` with the facets above | 200 | `total_hits: 46`; first hit `project_id: AANobbMI`, `slug: sodium` |
| `/project/sodium/version`, Fabric + 1.21.1 | 200 | 22 entries; first `id: SMxNOGZ6`, `game_versions: ["1.21.1"]`, `loaders: ["fabric"]` |
| `/version/SMxNOGZ6` | 200 | `id: SMxNOGZ6` |
| `/version_file/{sha1}?algorithm=sha1` | 200 | `id: SMxNOGZ6` |
| `/version_file/{sha512}?algorithm=sha512` | 200 | `id: SMxNOGZ6` |
| `POST /version_files`, body above | 200 | Input SHA-1 mapped to version `SMxNOGZ6` |
| `POST /version_files/update`, body above | 200 | Input SHA-1 mapped to version `SMxNOGZ6` |

These are reduced projections, not complete bodies. The update result equaled the input version; it proves successful lookup, not availability of a newer release. Unknown hashes, multiple matches, malformed requests, and rate-limit exhaustion were **UNVERIFIED** in live tests.

The LIVE project response also contained `"client_side":"required"` and `"environment":["client_only"]`. That array-valued environment differs from the scalar project schema in the documentation. Normalize deliberately and preserve unknown shapes.

Actual project response headers included:

```text
X-Ratelimit-Limit: 300
X-Ratelimit-Remaining: 275
X-Ratelimit-Reset: 4
```

Primary API URLs: [project](https://api.modrinth.com/v2/project/sodium), [version](https://api.modrinth.com/v2/version/SMxNOGZ6), [SHA-1 lookup](https://api.modrinth.com/v2/version_file/003c114c85ca88ef3362e018deb6aca0c682d6a1?algorithm=sha1), [search](https://api.modrinth.com/v2/search?query=sodium&limit=1). Counts and newest versions are snapshots.

## 1. Modrinth API v2

### Transport, identity, and limits

Use `https://api.modrinth.com/v2`. Public catalog reads normally require no token. Each request must identify the application in `User-Agent`; there is no mandatory rigid string grammar. A suitable release template is `OWNER/NexoraLauncher/VERSION (REAL_CONTACT_URL_OR_EMAIL)`. Replace placeholders before release. Contact information is recommended, not required; identifying only the HTTP library is inadequate.

The documented allowance is **300 requests/minute per IP**, identical with or without authentication. `X-Ratelimit-Limit` is the minute allowance, `X-Ratelimit-Remaining` the remaining count, and `X-Ratelimit-Reset` seconds until reset, not a Unix timestamp. These headers were also observed live above. Proposed policy: share a Rust-side limiter, batch queries, cache appropriate public metadata, and back off on 429 using server reset/retry guidance and jitter. Actual 429 responses remain **UNVERIFIED**. [API overview](https://docs.modrinth.com/api/)

### Search and content classification

`GET /search` accepts `query`, JSON-encoded `facets`, `offset`, `limit` (maximum 100), and sorting `index`. Inner arrays are OR; separate arrays are AND. Loaders use `categories`, not a separate `loaders` search facet:

```json
[
  ["project_type:mod"],
  ["categories:technology","categories:adventure"],
  ["categories:fabric","categories:quilt"],
  ["versions:1.21.1"],
  ["client_side:required","client_side:optional"]
]
```

This combines type, category alternatives, loader alternatives, game version, and legacy client support. Current documentation deprecates `client_side`/`server_side` in favor of `environment`; legacy values include `required`, `optional`, `unsupported`, and `unknown`. A newer filter can use `environment:client_only`. Define which environments Nexora supports; client-installable content is not necessarily exclusively client-only. Search discovers projects; recheck the chosen version before installing. [Search reference](https://docs.modrinth.com/api/operations/searchprojects/)

LIVE [`/tag/project_type`](https://api.modrinth.com/v2/tag/project_type) returned:

```json
["mod","modpack","resourcepack","shader","plugin","datapack","minecraft_java_server"]
```

The older project schema lists only `mod`, `modpack`, `resourcepack`, and `shader`. Use extensible types; plugin/server software need not become initial launcher features merely because the catalog lists them. [Project schema](https://docs.modrinth.com/api/operations/getproject/), [type reference](https://docs.modrinth.com/api/operations/projecttypelist/)

LIVE [`/tag/loader`](https://api.modrinth.com/v2/tag/loader) identified `iris`, `optifine`, and `canvas` as shader loaders supporting `shader`. Search shaders with `project_type:shader` and an OR group of those category tags. A LIVE query returned `complementary-reimagined`, labeled `shader`, with `iris` and `optifine` categories. These describe shader compatibility, not authorization to automatically install shader-engine binaries. [Loader reference](https://docs.modrinth.com/api/operations/loaderlist/)

Datapacks now have a project-type tag, while `datapack` also remains a loader/category tag. The LIVE loader entry supported `datapack`, `project`, and `mod`. A `project_type:datapack` search returned `veinminer` with hit field `project_type:mod`; legacy `project_type:mod` plus `categories:datapack` also returned results. Project labels alone are insufficient to select artifact roles. Proposed policy: distinguish search classifications, version loaders, and file destinations. Datapacks need a world/datapacks destination or a supported datapack-loading integration; do not put all datapacks into `mods/`. [LIVE datapack search](https://api.modrinth.com/v2/search?facets=%5B%5B%22project_type%3Adatapack%22%5D%5D&limit=1)

### Projects, versions, hashes, updates

- `GET /project/{id|slug}` supplies metadata. Persist stable IDs because slugs can change. Team/gallery endpoints are outside initial scope. [Project](https://docs.modrinth.com/api/operations/getproject/)
- `GET /project/{id|slug}/version?loaders=["fabric"]&game_versions=["1.21.1"]` lists filtered versions. URL-encode both JSON strings. `include_changelog=false` reduces payloads. [Version list](https://docs.modrinth.com/api/operations/getprojectversions/)
- `GET /version/{id}` retrieves an exact release, including compatibility, dependencies, and `files` with `hashes`, `url`, `filename`, `size`, and `primary`. The documented default is the first file if none is primary. Proposed policy: avoid sources/dev/Javadoc artifacts and expose ambiguous choices. [Version](https://docs.modrinth.com/api/operations/getversion/)
- `GET /version_file/{hash}?algorithm=sha1|sha512` identifies the containing version. Hash complete original bytes and encode in hex; specify the algorithm explicitly. The reference also documents `multiple`. Identification is not an update check. [Single hash](https://docs.modrinth.com/api/operations/versionfromhash/)
- `POST /version_files` takes `{"hashes":["HEX"],"algorithm":"sha512"}` and returns a hash-to-version map. Treat missing mappings as unknown, not filename guesses. [Bulk hashes](https://docs.modrinth.com/api/operations/versionsfromhashes/)
- `POST /version_files/update` takes `hashes`, `algorithm`, `loaders`, and `game_versions`; optional `version_types`, such as `["release"]`, restricts channels. Results are keyed by original installed hashes. Compare candidate IDs/files, then resolve dependencies. Batch by instance context; broad game-version filters can offer unwanted migrations. [Bulk updates](https://docs.modrinth.com/api/operations/getlatestversionsfromhashes/)

Proposed download policy: choose the intended artifact, verify hashes and size, and retain provider/project/version/hash provenance. A hash match establishes identity, not instance compatibility or freedom from malicious behavior.

### Dependency records

Version dependency records contain nullable `version_id`, `project_id`, and `file_name`, plus `dependency_type`: `required`, `optional`, `incompatible`, or `embedded`. Version IDs pin exact releases; project IDs leave release selection to the client. A filename can describe an external pack dependency but is not a resolvable provider ID. These fields do not offer a general version-range expression. [Dependency schema](https://docs.modrinth.com/api/operations/getversion/)

Proposed interpretation: install required edges, offer optional edges, enforce incompatible edges as negative constraints, and avoid downloading embedded components separately merely because they appear in the graph. When both IDs exist, verify the exact version belongs to the stated project. When neither exists, preserve the description and require explicit resolution if needed. This is Nexora policy, not a guarantee of complete uploader metadata.

## 2. Modrinth `.mrpack`

The [official specification](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack) defines ZIP storage with UTF-8 `modrinth.index.json` at the root:

| Field | Shape / meaning |
| --- | --- |
| `formatVersion` | Number; currently `1` |
| `game` | `"minecraft"` |
| `versionId` | Pack release identifier |
| `name`, `summary` | Name; optional summary |
| `files` | Download records |
| `files[].path` | Instance-relative destination |
| `files[].hashes` | Both `sha1` and `sha512` required |
| `files[].env` | Optional `client`/`server`: `required`, `optional`, `unsupported` |
| `files[].downloads` | HTTPS URL candidates |
| `files[].fileSize` | Bytes |
| `dependencies` | Version strings: `minecraft`, `forge`, `neoforge`, `fabric-loader`, `quilt-loader` |

Copy `overrides/` into the instance, then the applicable `client-overrides/` or `server-overrides/`, overwriting common files. Server means dedicated server. Offer optional files; omit unsupported-side files. Handle unknown dependency keys explicitly.

Modrinth upload validation permits download hosts `cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com`, and `gitlab.com`. Implementers should define an allowlist and follow bounded redirects; supporting at least three redirects is recommended. This host list does not grant content redistribution rights.

### Proposed Nexora import defenses

1. Validate every manifest path and ZIP member in every overrides layer. Reject absolute/rooted paths, drive-relative paths like `C:foo`, drive prefixes, UNC/device paths, and parent components under either slash convention. On Windows reject alternate data streams, reserved device names, trailing-dot/space ambiguities, and case-normalized collisions.
2. Resolve destinations beneath a controlled staging root and verify containment by path components. String-prefix checks are insufficient. Reject symlink/hard-link entries and Windows reparse-point escapes, including existing parent directories.
3. Bound entry count, decompressed size, compression ratio, JSON size, download size, and redirect count. Treat remote filenames as untrusted independently of manifest paths.
4. Require HTTPS and exact hostname matching. Validate each redirect target and deny loopback/private/link-local destinations, embedded credentials, and unsafe port changes. GitHub/GitLab downloads can redirect to asset hosts; define those hosts explicitly. Real-pack redirect behavior remains **UNVERIFIED** here.
5. Verify downloaded bytes before applying overrides. Define deterministic collisions across download and override layers. If an override replaces a downloaded file, record its final provenance instead of claiming the original hash still describes it.
6. Stage changes transactionally. Reject unsupported format/game/runtime requirements instead of producing a partially playable instance. Do not execute bundled scripts just because they are present.

`versionId` should be treated as a pack-local identifier, not assumed to be an API version ID. Preserve the distinction between archive metadata and API identity.

## 3. Modrinth terms for third-party launchers

The [Terms of Use](https://modrinth.com/legal/terms), last modified August 13, 2026, expressly cover the API. Their API section grants a limited, nonexclusive, nonsublicensable, revocable right to interact with user content through one's application, subject to the terms, community standards, law, and third-party rights. Assistance and indemnification obligations also apply. This supports third-party integrations, not ownership of hosted content or unconditional access.

The terms reserve service intellectual property and trademarks, and contain personal/noncommercial-use wording outside the API section. **UNVERIFIED:** its application to a commercial Nexora offering; obtain written clarification before monetization or branded endorsement. Read project licenses and preserve links. Apache-2.0 for the launcher does not relicense downloaded content. API access should not be assumed to permit mirroring/rehosting. The official [modpack guide](https://support.modrinth.com/en/articles/8802250-modpacks-on-modrinth) acknowledges third-party tools.

## 4. CurseForge Core API

### Access, keys, and terms

Use `https://api.curseforge.com`, catalog routes under `/v1`, and `x-api-key` authentication. Apply through the form linked in the official [application guide](https://support.curseforge.com/support/solutions/articles/9000208346-about-the-curseforge-api-and-how-to-apply-for-a-key), supplying contact/project information, expected usage, and acceptance of third-party terms. Overwolf reviews applications and contacts approved developers. Creating a game-studio console account is not equivalent to third-party launcher approval.

The [third-party API terms](https://support.curseforge.com/support/solutions/articles/9000207405-curseforge-3rd-party-api-terms-and-conditions), section 2.2, prohibit transferring/disclosing keys except to qualifying employees. There is no general OSS embedding exception established there. **Conclusion/inference:** do not publish a key in source or assume a recoverable key embedded in desktop binaries is authorized. **UNVERIFIED:** permission for Nexora to embed/distribute a key; obtain a written arrangement.

These terms also restrict caching API data, competing services, circumvention, and concealing identity/location through proxies/VPNs. Quotas may change and require a separate paid agreement. Confirm Nexora's intended cache and backend architecture in its agreement; do not inherit Modrinth policies automatically. The game-studio terms bundled with the REST reference are not a substitute for third-party terms.

### Catalog operations

Documented routes from the [Core REST reference](https://docs.curseforge.com/rest-api/):

| Purpose | Route / input |
| --- | --- |
| Classes | `GET /v1/categories?gameId=432&classesOnly=true` |
| Search | `GET /v1/mods/search?gameId=432&classId=6&gameVersion=1.21.1&modLoaderType=4&searchFilter=...` |
| Project | `GET /v1/mods/{modId}`; bulk `POST /v1/mods` with `modIds` |
| Files | `GET /v1/mods/{modId}/files`, with game/loader filters |
| Exact file | `GET /v1/mods/{modId}/files/{fileId}` |
| Bulk files | `POST /v1/mods/files` with `fileIds` |
| Download URL | `GET /v1/mods/{modId}/files/{fileId}/download-url` |
| Fingerprints | `POST /v1/fingerprints/432` or `/v1/fingerprints`; `{"fingerprints":[NUMBER]}` |

Search uses `index`/`pageSize`; maximum page size is 50, with `index + pageSize <= 10000`. Preserve `data` wrappers and pagination. File metadata includes hashes, compatibility, dependencies, and fingerprint. Loader enum examples: Forge `1`, Fabric `4`, Quilt `5`, NeoForge `6`.

**UNVERIFIED catalog constants:** Minecraft Java `gameId=432`; class IDs below are working candidates, not authenticated discoveries here. Confirm through categories before shipping. Class IDs, child categories, and loader enums are distinct.

| Content | Candidate `classId` | Status |
| --- | --- | --- |
| Mods | `6` | UNVERIFIED |
| Modpacks | `4471` | UNVERIFIED |
| Resource/texture packs | `12` | UNVERIFIED |
| Shaders | `6552` | UNVERIFIED |
| Datapacks | `6945` | UNVERIFIED |
| Worlds | `17` | UNVERIFIED |
| Bukkit plugins | `5` | UNVERIFIED; outside initial scope |

The public reference did not establish a fixed requests/minute quota or standard rate-header contract. **UNVERIFIED:** any universal numeric rate limit. Proposed policy: configure limits from Nexora's agreement, bound concurrency, and handle 429 with server guidance; do not assume Modrinth's 300/minute.

### Fingerprint matching

The CurseForge variant computes MurmurHash2 with seed `1` after removing precisely bytes `0x09`, `0x0A`, `0x0D`, and `0x20` throughout the original archive bytes. This is byte filtering, not Unicode whitespace handling, ZIP extraction, or SHA hashing. The Murmur input length is the filtered length. Send the unsigned 32-bit result as a JSON number.

This algorithm description was checked against the primary [packwiz fingerprint implementation](https://github.com/packwiz/packwiz/blob/main/curseforge/murmur2/hash.go); no implementation code is reproduced. The official REST reference documents matching endpoints but supplied no normative algorithm specification. **UNVERIFIED:** a future Nexora implementation's agreement with live server fingerprints. Establish known-file test vectors. Prefer exact matches; neither fuzzy matches nor a 32-bit fingerprint provide cryptographic integrity. Verify stronger available file hashes and size separately.

### Distribution restrictions and manual downloads

`allowModDistribution` is nullable. The official [distribution-toggle guide](https://support.curseforge.com/support/solutions/articles/9000207877-project-distribution-toggle) says authors control availability of project information/files to third-party integrations. A false flag is not permission to try another CDN path. Proposed policy: null means unknown, not consent.

`downloadUrl` may be absent/null or retrieval may fail. **UNVERIFIED:** exact null/error combinations for restricted, deleted, unavailable, or permission-limited files under Nexora's future key. Do not infer one cause from null. Where authorized, use the documented download-URL endpoint; never construct alternate CDN URLs to evade restrictions.

Proposed manual-download flow: retain project/file IDs, show filename and the official file page, let the user download through the provider website, then accept an explicitly selected local file and validate size/hash. Required files stay pending and block completion; optional files may be skipped. Do not silently substitute versions or mirrors. This is Nexora's proposed handling; specific third-party launcher implementations were **UNVERIFIED**, since their source was not inspected.

### 2026 CDN authentication change

An official [June 10, 2026 announcement](https://blog.curseforge.com/introducing-api-key-authentication-for-curseforge-file-downloads/) says direct downloads from `edge.forgecdn.net` require API keys from July 16, 2026, with missing/invalid keys yielding 401. It supports `x-api-key` (preferred) or the `api-key` query parameter. The announced date has passed. **UNVERIFIED:** actual enforcement for a real artifact and Nexora key.

Design for authenticated downloads as well as authenticated metadata. Avoid keys in logged URLs. Attach credentials only to approved provider hosts, never blindly across redirects. This notice does not itself waive key-disclosure restrictions or author distribution choices.

## 5. CurseForge modpack ZIP format

The official [export guide](https://support.curseforge.com/support/solutions/articles/9000197908-exporting-a-modpack-for-curseforge-project-submission) establishes a ZIP with root `manifest.json` and `overrides/`. Overrides contain configuration and other bundled files; CurseForge-hosted mods should be manifest references, not copied into the archive.

Expected structure, as an original illustration rather than a downloaded pack:

```json
{
  "minecraft": {
    "version": "1.21.1",
    "modLoaders": [{"id": "fabric-0.16.10", "primary": true}]
  },
  "manifestType": "minecraftModpack",
  "manifestVersion": 1,
  "name": "Example pack",
  "version": "1.0.0",
  "author": "Example author",
  "files": [{"projectID": 123456, "fileID": 789012, "required": true}],
  "overrides": "overrides"
}
```

`minecraft.modLoaders` supplies loader identifiers and primary selection. `files` pins exact numeric `projectID`/`fileID` pairs; `required` controls whether omission is permitted. `overrides` names the directory copied into the instance. A launcher maintainer's [manifest documentation](https://gdlauncher.com/docs/modpack-manifest-format/) also describes this structure. The official [processor errors guide](https://support.curseforge.com/support/solutions/articles/9000210425-curseforge-file-processor-rejections-and-how-to-solve-them) confirms loader validation and duplicate project references matter.

**UNVERIFIED:** this example was not imported or checked against a live exported archive; its IDs are illustrative. Support for every contemporary loader prefix also remains unverified. Resolve loader IDs through a dedicated installer, not guessed runtime URLs. Fetch exact file metadata and honor manual-download restrictions. Apply the same path/extraction defenses as `.mrpack`, including validation of the manifest-controlled overrides directory. Do not replace pinned file IDs with newest releases.

## 6. Dependency resolution for an instance

This proposed Nexora algorithm uses the [Modrinth version model](https://docs.modrinth.com/api/operations/getversion/); Modrinth is not supplying a complete solver.

1. **Fix context:** Minecraft version, loader family/version, physical side, release channels, and content role. Check version-level compatibility after search. Fabric/Quilt and Forge/NeoForge compatibility require explicit policy, not string aliasing. Shader loaders and vanilla/resource-pack roles differ.
2. **Honor exact pins:** fetch `version_id`, verify project/game/loader/environment, and report contradictions instead of replacing it with latest. Pack and user pins constrain updates.
3. **Choose project-only dependencies:** fetch versions filtered by game and loader, exclude disallowed channels/artifacts, and prefer an existing compatible selection. Otherwise propose the newest eligible publication with a deterministic tie-breaker. Do not sort arbitrary version strings as semantic versions. Keep alternatives for backtracking; record the final choice as an exact pin.
4. **Build a graph:** use provider-qualified IDs, deduplicate shared dependencies, and retain the parent/reason for every edge. Expand required edges and selected optional edges. Embedded edges ordinarily need no separate download; preserve them for provenance/conflict checks where metadata permits.
5. **Handle cycles:** use visiting/visited states or strongly connected components. A compatible A-to-B-to-A cycle is not automatically invalid; resolve the group once without recursion loops. Contradictory pinned cycles are errors. Bound graph size/depth/fetch counts.
6. **Apply negative constraints:** project-level `incompatible` forbids that project's presence; version-specific edges forbid the exact version. If both IDs exist, verify the relationship and retain the exact target. Check selected and installed content in both directions. Explain conflicts; do not silently remove user mods. Two required exact pins cannot be satisfied by installing duplicate JARs.
7. **Reject ambiguity:** missing required targets, inaccessible versions, conflicting pins, unknown required dependency kinds, and runtime contradictions block a valid plan. Filename-only external dependencies need manual resolution or explicit pack evidence.
8. **Validate the final set:** recheck after optional choices and updates, select installable files, verify downloads, and record hashes/paths/provider IDs/reasons. Do not equate projects across providers by name. Bulk update candidates pass through the same resolver.

Uploader API metadata may not enumerate every constraint in a JAR's loader manifest or every runtime incompatibility. Proposed policy: eventually supplement provider metadata with loader metadata, keeping provider declarations, file declarations, and user overrides distinct. Complete runtime compatibility remains **UNVERIFIED** until tested in Minecraft.

## Provider abstraction recommendation

Use provider-qualified identifiers and capabilities for search, exact versions, identification by hash/fingerprint, update checks, and download availability. Preserve original provider data alongside normalized game/loader/content-role fields. Represent downloads as available, manual action required, unavailable, or authentication required; not every file has a public URL.

Keep terms, credentials, rate handling, and cache rules provider-specific. Share safe extraction, runtime installation, integrity checks, and transactional instance operations. Modrinth comes first; CurseForge remains a later integration pending approved key use and agreement review. These are recommendations only, with no product code.

## Implementation checklist

- [ ] Repeat HTTPS curl probes with working TLS and retain successful statuses, headers, and short bodies. Current curl verification remains **UNVERIFIED**.
- [ ] Configure a unique release User-Agent with real project/contact details.
- [ ] Use provider-qualified stable IDs, capability flags, extensible metadata parsing, and provider-specific cache/rate policies.
- [ ] Support Modrinth facets, encoded version filters, exact versions, both hash algorithms, bulk identification, and bulk updates.
- [ ] Enforce version/file compatibility, channels, user pins, and pack pins after discovery.
- [ ] Resolve required/optional/embedded/incompatible edges with deduplication, cycle handling, deterministic choices, and useful conflict explanations.
- [ ] Parse `.mrpack` version 1, physical-side choices, documented runtime dependency keys, and common-then-side overrides.
- [ ] Enforce safe paths, archive/download limits, redirect policies, hashes, and staged installation.
- [ ] Define datapack destinations and shader-engine installation policy before automatic installs.
- [ ] Review Modrinth terms, project licenses, attribution/link presentation, and commercial-use clarification if applicable.
- [ ] Apply for Nexora's own CurseForge access; obtain written approval for key distribution, cache behavior, and backend architecture.
- [ ] Verify authenticated CurseForge catalog constants, loader enums, quotas, denied/null downloads, and 2026 CDN behavior.
- [ ] Validate whitespace-filtered Murmur2 against known-file server fingerprints and stronger hashes.
- [ ] Inspect real exports of both pack formats and verify manual-download completion without changing pins.
- [ ] Preserve the planned Apache-2.0 boundary; do not copy GPL launcher code or assume downloaded content shares the launcher license.

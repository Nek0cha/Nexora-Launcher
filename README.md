# Nexora Launcher

A modern, lightweight, open-source launcher for Minecraft: Java Edition, built with Rust and Tauri 2.

## About

Nexora Launcher installs and launches official Minecraft: Java Edition versions. Builds for Windows, macOS and Linux are published on the [Releases](https://github.com/Nek0cha/Nexora-Launcher/releases) page.

The project is in early development. Building the Microsoft sign-in needs a client ID in the `NEXORA_MS_CLIENT_ID` environment variable; forks must register their own and pass Minecraft API review (aka.ms/mce-reviewappid).

- Done: installing and launching vanilla game versions, including the Java runtime
- Microsoft sign-in is implemented; the Minecraft API review was passed on 2026-10-17
- Instances can be created, duplicated, deleted and configured (memory, Java, JVM arguments), and the skin can be changed
- A running instance can be stopped from the launcher (a graceful close first, with a force-stop prompt if it does not exit); the app has its own title bar, toast notifications, a memory slider, and buttons that open an instance's folders
- Fabric and Quilt instances can be created, installed and launched
- Forge (Minecraft 1.8.9–1.12.2, 1.16.5–1.21.x and 26.x) and NeoForge (1.20.1–1.21.x and 26.x) instances can be created, installed and launched by the code. They are implemented and covered by automated tests that use a fake Java; verification on a real machine is still pending
- An instance's content tab lists its mods, resource packs and shaders, which can be enabled, disabled, removed, filtered, and added by drag and drop; files are identified on Modrinth by their hash
- A Modrinth modpack (`.mrpack`) can be imported as a new instance (picked from a dialog or dropped onto the window), and an instance can be exported as a `.mrpack`. Files that Modrinth identifies are written as references; any others are bundled only after the user confirms they may be redistributed
- Instances of Modrinth App can be imported by choosing, per instance, whether to copy its mods, screenshots, worlds and settings (`config/`, `options.txt`, `servers.dat`). The original data is only read, never changed, and an instance that looks like one you already have is reported before anything is copied.
- Instances of Prism Launcher, the official Minecraft launcher and the CurseForge app can be imported the same way. Only Modrinth App has been tried against real data; the other three were written from public documentation and source and are untested on real installs (the CurseForge app in particular was never installed during development).
- Planned: content from CurseForge

## Install

Download the file for your system from the [Releases](https://github.com/Nek0cha/Nexora-Launcher/releases) page.

| System | File |
|---|---|
| Windows | `Nexora_<version>_x64-setup.exe` |
| macOS (Apple Silicon) | `Nexora_<version>_aarch64.dmg` |
| macOS (Intel) | `Nexora_<version>_x64.dmg` |
| Linux | `Nexora_<version>_amd64.AppImage` or `Nexora_<version>_amd64.deb` |

The installers are not code-signed, because a signing certificate costs money and Nexora Launcher is a free project. The first launch therefore shows a warning from the operating system:

- **Windows:** SmartScreen shows "Windows protected your PC". Choose "More info", then "Run anyway".
- **macOS:** the app is blocked as coming from an unidentified developer. Open it once, then go to System Settings → Privacy & Security and choose "Open Anyway" next to the message about Nexora. If macOS says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/Nexora.app` in a terminal.
- **Linux:** make the AppImage executable (`chmod +x`) and run it. Only the AppImage updates itself; with the `.deb`, install the new package by hand.

The app checks for a new version when it starts, and shows an update button in the title bar when there is one. Updates are verified with a signing key of the project; nothing is installed until you press the button.

### Accounts and privacy

- Players sign in with their own Microsoft account in the system browser. The launcher uses the authorization code flow with PKCE, and does not use a client secret.
- There is no backend server. All authentication happens on the player's device.
- When an instance's content tab is opened, the launcher sends the SHA-1 hashes of the installed mods, resource packs and shaders to api.modrinth.com to identify them. These are read-only lookups and include no account data.
- When content is searched, installed or updated, the launcher also contacts Modrinth: the search text and filters (Minecraft version, loader, categories, environment) go to api.modrinth.com; version lookups for installs and their dependencies go to api.modrinth.com; update checks send the SHA-1 hashes of the installed mods together with the instance's Minecraft version and loader; and files are downloaded from cdn.modrinth.com. Nothing else is sent, and none of it includes account data.
- Importing a `.mrpack` also sends the SHA-1 hashes of the imported mods, resource packs and shaders to api.modrinth.com to identify them. It downloads its files only from cdn.modrinth.com, github.com, raw.githubusercontent.com, gitlab.com and GitHub's release-asset hosts, over HTTPS with at most three redirects, and rejects any file whose SHA-1, SHA-512 or size differs from the pack's index. Exporting sends the SHA-1 hashes of the enabled mods, resource packs and shaders to api.modrinth.com to look up their download URLs. Nothing else is sent, and none of it includes account data.
- Importing from Modrinth App sends nothing by itself; the imported mods are then identified like those of any instance, by sending their SHA-1 hashes to api.modrinth.com.
- At startup, and when you press "Check for updates" in Settings, the launcher requests `latest.json` from github.com (the project's release page). Only the request itself is sent; it contains no account data.
- The Microsoft refresh token is stored only in the operating system's credential store (Windows Credential Manager, macOS Keychain, or Secret Service on Linux).
- Tokens are sent only to Microsoft, Xbox, and Minecraft services, and are never written to logs.
- Release builds require an account that owns Minecraft: Java Edition.

Nexora Launcher is not an official Minecraft product and is not approved by or associated with Mojang or Microsoft.

## 概要（日本語）

Minecraft: Java Edition 用のオープンソースのランチャーです。現在は開発の初期段階です。Windows、macOS、Linux 向けの配布物を [Releases](https://github.com/Nek0cha/Nexora-Launcher/releases) で公開しています。

- インスタンスの一覧、作成、複製、削除ができます。
- インスタンスごとに、メモリ、Java、JVM 引数を設定できます。
- アカウント画面からスキンを変更できます（Minecraft API の審査は 2026-10-17 に通っています）。
- 起動中のインスタンスを、ランチャーから停止できます（まず閉じるよう頼み、終了しないときは強制終了の確認を出します）。
- 独自のタイトルバー、トースト通知、メモリのスライダー、インスタンスのフォルダを開くボタンがあります。
- インスタンスの詳細にコンテンツタブがあり、MOD、リソースパック、シェーダーを一覧できます。
  - 有効と無効を切り替えられます（ファイル名に `.disabled` を付け外しします）。ファイルのドラッグ＆ドロップで追加でき、削除もできます。
  - 名前、クライアント／サーバー、カテゴリ、状態で絞り込めます。MOD、リソースパック、シェーダーは、ファイルのハッシュから Modrinth 上の情報を調べて表示します。
  - コンテンツタブを開くと、入っている MOD、リソースパック、シェーダーのファイルの SHA-1 ハッシュを、それが何かを調べるために api.modrinth.com へ送ります（情報を読み取るだけで、アカウントの情報は送りません）。
  - Modrinth で MOD を検索し、依存する MOD とあわせて導入できます。更新の確認と適用もでき、更新の前には置き換える jar を退避します。
  - MOD を検索、導入、更新するときは、Modrinth に次の内容を送ります。検索の文字列と絞り込みの条件（Minecraft のバージョン、ローダー、カテゴリ、環境）、導入と依存の解決のための版の問い合わせ、更新の確認のための MOD のファイルの SHA-1 ハッシュと、インスタンスの Minecraft のバージョンとローダーです。ファイルは cdn.modrinth.com からダウンロードします。これ以外は送りません。アカウントの情報も送りません。
  - Modrinth のモッドパック（`.mrpack`）を、新しいインスタンスとして読み込めます（ファイル選択か、ウィンドウへのドロップ）。インスタンスを `.mrpack` として書き出すこともできます。Modrinth で特定できたファイルは参照として書き、特定できなかったファイルは、配布してよいか確認したうえで同梱します。
  - `.mrpack` を読み込むときは、入った MOD、リソースパック、シェーダーのファイルの SHA-1 ハッシュを、それが何かを調べるために api.modrinth.com へ送ります。ファイルは cdn.modrinth.com、github.com、raw.githubusercontent.com、gitlab.com と GitHub のリリースの配信先からだけ、HTTPS でファイルをダウンロードします（リダイレクトは 3 回まで）。SHA-1、SHA-512、大きさのどれかが索引と違うファイルは受け入れません。書き出すときは、有効な MOD、リソースパック、シェーダーの SHA-1 ハッシュを、ダウンロード元の URL を調べるために api.modrinth.com へ送ります。これ以外は送りません。アカウントの情報も送りません。
  - Modrinth App のインスタンスを、MOD、スクリーンショット、セーブデータ、設定（config、options.txt、servers.dat）のどれを持ってくるかを選んで読み込めます。元のデータは読むだけで変更せず、すでにあるインスタンスと重複しそうなときはコピーの前にお知らせします。Prism Launcher、公式ランチャー、CurseForge アプリのインスタンスも同じ形で読み込めますが、実際のデータで試したのは Modrinth App だけです（ほかの 3 つは公開されている資料から書いたもので、CurseForge アプリは開発中に一度も入れていません）。
  - Modrinth App から読み込むこと自体は、何も送信しません。読み込んだ MOD は、ほかのインスタンスと同じく、それが何かを調べるために SHA-1 ハッシュを api.modrinth.com へ送ります。

## 導入（日本語）

[Releases](https://github.com/Nek0cha/Nexora-Launcher/releases) から、お使いの環境のファイルをダウンロードしてください（Windows は `-setup.exe`、macOS は `.dmg`、Linux は `.AppImage` か `.deb`）。

配布物にはコード署名をしていません（署名の証明書は有料で、このプロジェクトは無料で公開しているためです）。そのため、初回の起動で OS が警告を出します。

- **Windows：** SmartScreen が「Windows によって PC が保護されました」と表示します。「詳細情報」を押して、「実行」を選んでください。
- **macOS：** 開発元を確認できないアプリとして止められます。いちど開いてみたあと、「システム設定 → プライバシーとセキュリティ」で、Nexora についての表示の横にある「このまま開く」を押してください。「壊れているため開けません」と出たときは、ターミナルで `xattr -dr com.apple.quarantine /Applications/Nexora.app` を実行してください。
- **Linux：** AppImage に実行権限を付けて（`chmod +x`）起動してください。自動で更新されるのは AppImage だけです。`.deb` は、新しいパッケージを手で入れ直してください。

アプリは起動したときに新しいバージョンを確認し、あればタイトルバーに更新のボタンを出します。更新はプロジェクトの署名鍵で検証され、ボタンを押すまでは何もインストールしません。更新の確認では、github.com へリクエストを送るだけで、アカウントの情報は送りません。

## 開発環境

- Rust 1.96（`rust-toolchain.toml` で自動的に選ばれます）
- Node.js 24
- pnpm 11（`corepack enable` で `package.json` の版が使われます）
- Windows では WebView2（Windows 11 には標準で入っています）
- Linux では Tauri 2 の前提パッケージ（https://v2.tauri.app/start/prerequisites/ ）

## 起動

```sh
pnpm install
pnpm dev
```

開発ビルドは、識別子 `net.ny4n.nexora.dev` と、データフォルダ `Nexora-dev`（Windows では `%APPDATA%\Nexora-dev`）を使います。製品版のデータには触れません。

### 開発用のデータフォルダ

開発ビルドでは、環境変数 `NEXORA_DATA_DIR` でデータフォルダを指定できます（リリースビルドでは無視されます）。Windows の PowerShell では次のように指定します。

```powershell
$env:NEXORA_DATA_DIR = "C:\Users\me\日本語のフォルダ\nexora"; pnpm dev
```

macOS と Linux では次のように指定します。

```sh
NEXORA_DATA_DIR="$HOME/日本語のフォルダ/nexora" pnpm dev
```

### Microsoft ログインのクライアント ID

Microsoft のアカウントでログインするには、ビルド時に環境変数 `NEXORA_MS_CLIENT_ID` でクライアント ID を指定します。Windows の PowerShell では次のように指定します。

```powershell
$env:NEXORA_MS_CLIENT_ID = "<クライアント ID>"; pnpm dev
```

macOS と Linux では次のように指定します。

```sh
NEXORA_MS_CLIENT_ID="<クライアント ID>" pnpm dev
```

値が空の場合は未設定として扱います。指定されていないビルドでは、「Microsoft でログイン」のボタンを押すと「このビルドには Microsoft のクライアント ID が含まれていないため、ログインできません」（`auth.no_client_id`）と表示されます。このときも、開発用のオフラインアカウントでは起動できます。

フォークして配布する場合は、ご自身でクライアント ID を取得し、Minecraft API の利用について審査（aka.ms/mce-reviewappid）を受ける必要があります。

### 開発用のオフラインアカウント

開発ビルドでは、アカウント画面の「開発用のオフラインアカウント」の行で使うを選ぶと、そのアカウントで起動できます。名前はアカウント画面で変更できます（既定は `Player`）。リリースビルドでは起動できません。

## テスト

```sh
cargo test --workspace
pnpm --filter ui test --run
pnpm --filter ui typecheck
```

`cargo test` を実行すると、Rust の型から TypeScript の型（`ui/src/bindings/`）が生成されます。生成されたファイルはコミットしてください。

## ドキュメント

- 全体設計：`docs/specs/2026-10-15-nexora-architecture-design.md`
- フェーズ 1 の設計：`docs/specs/2026-10-15-phase1-foundation-design.md`
- フェーズ 4 の設計：`docs/specs/2026-10-19-phase4-instance-ui-design.md`
- フェーズ 5 の設計：`docs/specs/2026-10-20-phase5-loaders-and-content-design.md`
- フェーズ 10 の設計：`docs/specs/2026-11-01-phase10-release-design.md`
- リリースの手順：`docs/releasing.md`
- 調査資料：`docs/research/`

## ライセンス

Apache License 2.0 です。詳しくは `LICENSE` を参照してください。同梱する第三者のソフトウェアの表示は `THIRD_PARTY_NOTICES.md` にあります。

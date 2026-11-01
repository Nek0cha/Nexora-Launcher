<div align="center">

<img src="src-tauri/app-icon.png" width="112" alt="Nexora Launcher のロゴ">

# Nexora Launcher

Minecraft: Java Edition 用の、軽くてモダンなオープンソースのランチャーです。<br>
Rust と Tauri 2 で作っています。

[![CI](https://github.com/Nek0cha/Nexora-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Nek0cha/Nexora-Launcher/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Nek0cha/Nexora-Launcher)](https://github.com/Nek0cha/Nexora-Launcher/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

[English](README.md) | **日本語**

</div>

## 概要

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

## 導入

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

Apache License 2.0 です。詳しくは `LICENSE` を参照してください。同梱する第三者のソフトウェアのライセンスは、アプリの設定の「このアプリについて」から見られます。

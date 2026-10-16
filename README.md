# Nexora Launcher

A modern, lightweight, open-source launcher for Minecraft: Java Edition, built with Rust and Tauri 2.

## About

Nexora Launcher installs and launches official Minecraft: Java Edition versions. Windows is the first target, and macOS and Linux are planned.

The project is in early development.

- Done: installing and launching vanilla game versions, including the Java runtime
- Next: signing in with a Microsoft account and managing several accounts
- Planned: skin changes, mod loaders (Fabric, Quilt, Forge, NeoForge), content from Modrinth and CurseForge, and importing instances from other launchers

### Accounts and privacy

- Players sign in with their own Microsoft account in the system browser. The launcher uses the authorization code flow with PKCE, and does not use a client secret.
- There is no backend server. All authentication happens on the player's device.
- The Microsoft refresh token is stored only in the operating system's credential store (Windows Credential Manager, macOS Keychain, or Secret Service on Linux).
- Tokens are sent only to Microsoft, Xbox, and Minecraft services, and are never written to logs.
- Release builds require an account that owns Minecraft: Java Edition.

Nexora Launcher is not an official Minecraft product and is not approved by or associated with Mojang or Microsoft.

## 概要（日本語）

Minecraft: Java Edition 用のオープンソースのランチャーです。現在は開発の初期段階です（フェーズ 3：Microsoft 認証とアカウント管理）。

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

### 開発用のオフラインアカウント

フェーズ 3（Microsoft 認証）までは、開発ビルドだけオフラインアカウントで起動できます。名前はアカウント画面で変更できます（既定は `Player`）。リリースビルドでは起動できません。

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
- 調査資料：`docs/research/`

## ライセンス

Apache License 2.0 です。詳しくは `LICENSE` を参照してください。

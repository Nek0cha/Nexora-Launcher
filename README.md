# Nexora Launcher

A modern, lightweight Minecraft Java Edition launcher.

現在は開発の初期段階です（フェーズ 2：Vanilla のインストールと起動が使えるようになりました）。

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

開発ビルドでは、環境変数 `NEXORA_DATA_DIR` でデータフォルダを指定できる（リリースビルドでは無視する）。

    NEXORA_DATA_DIR="C:\Users\me\日本語のフォルダ\nexora" pnpm dev

### 開発用のオフラインアカウント

フェーズ 3（Microsoft 認証）までは、開発ビルドだけオフラインアカウントで起動できる。名前はアカウント画面で変更できる（既定は `Player`）。リリースビルドでは起動できない。

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

未定です（Apache-2.0 を予定しています）。

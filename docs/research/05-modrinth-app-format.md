# Modrinth App の保存形式

フェーズ 8 の読み取り部分（`import/modrinth_app.rs`）のために、Modrinth App の公開リポジトリ（`modrinth/code`、`packages/app-lib`）から確かめた事実を記す。利用者の PC のフォルダは読んでいない。確認日は 2026-11-01。

## 確かめたこと

- **場所**：設定フォルダは `dirs::data_dir()/<app_identifier>`（環境変数 `THESEUS_CONFIG_DIR` があればそれ）。`app.db`（SQLite）はその直下にある。インスタンスのフォルダは `<config_dir>/profiles/<instance path>/` で、`config_dir` は既定では設定フォルダと同じだが、設定の `custom_dir` で変えられる。
- **ローダー**：文字列で `vanilla`、`forge`、`fabric`、`quilt`、`neoforge`（小文字）。知らない値は Vanilla として扱われる。
- **インスタンスの中身**：`mods`、`resourcepacks`、`shaderpacks` などが、インスタンスのフォルダの直下にある。`saves`、`screenshots` もゲームのフォルダとして同じ階層にある（Minecraft の標準）。
- **インスタンスのパス**：`instance path` は `profiles` の下の相対パスで、アプリ側が検証してから使っている（`..` などは断られる）。表示名（`name`）とは別。

## スキーマは 2 世代ある

- **旧**（初期のマイグレーションから）：`profiles` テーブルに `path`、`name`、`game_version`、`mod_loader`、`mod_loader_version`、`install_stage`。
- **新**（`20260611120000_instances-content-foundation`、`20260619120000_finish-instance-profile-migration` 以降）：`profiles` は削除され、`instances`（`id`、`path`、`applied_content_set_id`、`install_stage`、`name`）と、`instance_content_sets`（`id`、`instance_id`、`game_version`、`loader`、`loader_version`）に分かれた。インスタンスのローダーと版は、`instances.applied_content_set_id` が指す `instance_content_sets` の行にある。

読み取り部分は、`instances` テーブルがあれば新、無く `profiles` があれば旧として読む。どちらも無ければ `ImportUnavailable`。

## 確かめられなかったこと

- `app_identifier` の値。`ModrinthApp`（Windows では `%APPDATA%\ModrinthApp`）と考えているが、ソースで直接は確かめていない。見つからなければ利用者にフォルダを選んでもらう（spec 5.1）ので、誤っていても機能は止まらない。
- `install_stage` の取り得る値。完了済み以外（インストール途中）のインスタンスを一覧に出すかは、実装時に `unsupported` として扱う方針とする。
- 新しいスキーマで、ゲームのフォルダがインスタンスのフォルダの直下か、サブフォルダかは、`adapters/filesystem.rs` が直下の `mods` などを読むことまでしか確かめていない。

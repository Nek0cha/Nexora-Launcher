# リリースの手順

配布物は、`v` で始まるタグを push すると、GitHub Actions（`.github/workflows/release.yml`）が作る。設計は `docs/specs/2026-11-01-phase10-release-design.md` にある。

## 初めに一度だけ行うこと

1. 更新用の署名鍵を作る。

   ```sh
   pnpm tauri signer generate -w <リポジトリの外のパス>/nexora-updater.key
   ```

   パスワードを聞かれる。出力された公開鍵が `src-tauri/tauri.conf.json` の `plugins.updater.pubkey` に入っていることを確かめる。
2. GitHub のリポジトリの設定に、次を登録する。
   - Secrets：`TAURI_SIGNING_PRIVATE_KEY`（秘密鍵ファイルの中身）、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   - Variables：`NEXORA_MS_CLIENT_ID`（Microsoft サインインのクライアント ID。秘密ではない）
3. 秘密鍵とパスワードの控えを、リポジトリの外の安全な場所に保管する。

**秘密鍵を失うと、既に導入している利用者に更新を届けられなくなる。** 公開鍵を変えると、古い版のアプリは新しい版の署名を検証できず、利用者は手でインストーラを入れ直すことになる。

## リリースのたびに行うこと

1. 版番号を 3 か所で同じにする：`src-tauri/tauri.conf.json` の `version`、`Cargo.toml` の `[workspace.package] version`、これから打つタグ（`v` を除いた部分）。`node scripts/check-version.mjs` で確かめられる。`Cargo.lock` も更新される。
2. PR を作り、CI が通ったら `main` にマージする。
3. `main` の先頭に、タグを打って push する。

   ```sh
   git tag v0.1.1
   git push origin v0.1.1
   ```

4. Actions の `Release` が全部の OS で成功したら、GitHub の Releases に下書きができている。次を確かめる。
   - 配布物が揃っている：Windows の `-setup.exe`、macOS の `.dmg`（aarch64 と x64）と `.app.tar.gz`、Linux の `.AppImage` と `.deb`、それぞれの `.sig`、`latest.json`。
   - `latest.json` の `platforms` に、`windows-x86_64`、`darwin-aarch64`、`darwin-x86_64`、`linux-x86_64` がある。
5. 下書きを公開する（Publish release）。公開すると、`releases/latest/download/latest.json` がこの版を指し、古い版のアプリに更新の印が出る。

## 更新の動作確認

前の版のアプリを入れた状態で、新しい版を公開し、次を確かめる。

- 起動後にヘッダーへ更新のボタンが出る（設定の「このアプリについて」から手動で確認しても同じ）。
- 「更新して再起動」で、ダウンロード、インストール、再起動が済み、設定の「バージョン」が新しくなる。
- ゲームの起動中は、押せない。

## 既知の制約

- Windows と macOS はコード署名をしていないため、初回の起動で警告が出る（README に開き方を書いている）。
- Linux で自動更新できるのは AppImage だけである。
- 署名なしの macOS アプリが updater で置き換えられること、deb での挙動は、実機で確かめた結果をここに書き足す。

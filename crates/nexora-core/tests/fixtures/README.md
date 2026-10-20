# テスト用のサンプル

`versions/` は Mojang の version JSON の実物である（取得日：2026-10-15）。
`latest-release.json` は取得時点の最新の正式版で、ID は `26.3` である。

| ファイル | ID | sha1 |
|---|---|---|
| 1.8.9.json | 1.8.9 | d546f1707a3f2b7d034eece5ea2e311eda875787 |
| 1.12.2.json | 1.12.2 | 832d95b9f40699d4961394dcf6cf549e65f15dc5 |
| 1.13.json | 1.13 | c24c2fd37c8ca2e1c18721e2c77caf4d24c87f92 |
| 1.21.5.json | 1.21.5 | b341cd0e91419ab72b5d9dadad8d86a045d6b9a7 |
| latest-release.json | 26.3 | 11f1aefe473eefba0af62e3286c33b692807b2eb |

## loaders/

`loaders/` は Fabric と Quilt の公式メタデータの実物である（取得日：2026-10-20）。

| ファイル | 取得元 | 加工 |
|---|---|---|
| fabric-game.json | https://meta.fabricmc.net/v2/versions/game | 先頭の 6 件に切り詰めた |
| fabric-loader-1.20.1.json | https://meta.fabricmc.net/v2/versions/loader/1.20.1 | 先頭の 4 件に切り詰めた（先頭の 2 件は `stable: false`） |
| fabric-profile-1.20.1-0.16.10.json | https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.10/profile/json | なし |
| quilt-game.json | https://meta.quiltmc.org/v3/versions/game | 先頭の 6 件に切り詰めた |
| quilt-loader-1.20.1.json | https://meta.quiltmc.org/v3/versions/loader/1.20.1 | 先頭の 6 件に、`0.31.0-beta.4`、`0.28.0`、`0.28.1` の要素を元の順で足した |
| quilt-profile-1.20.1-0.28.1.json | https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.28.1/profile/json | なし |

Quilt の game 一覧は Fabric と同じ形（`version`、`stable`）である。
Quilt のゲームごとのローダー一覧は新しい順に並んでおらず、先頭は試験版（`0.20.0-beta.9`）である。

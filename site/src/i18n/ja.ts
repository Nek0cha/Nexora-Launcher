import type { Dict } from "./en";

export const ja: Dict = {
  meta: {
    title: "Nexora Launcher — オープンソースの Minecraft ランチャー",
    description:
      "Windows、macOS、Linux で動く、Minecraft: Java Edition 用のオープンソースのランチャーです。Rust と Tauri で作り、Apache-2.0 で公開しています。",
  },
  nav: { github: "GitHub", language: "English", languageHref: "/" },
  hero: {
    title: "コードごと​持っていける、​Minecraft​ランチャー。",
    lead: "Minecraft: Java Edition 用のオープンソースのランチャーです。Rust と Tauri で作り、Windows、macOS、Linux で動きます。ライセンスは Apache-2.0 で、GPL の依存はありません。",
    download: "ダウンロード",
    downloadFor: {
      windows: "Windows 用をダウンロード",
      "macos-arm": "macOS 用をダウンロード",
      "macos-intel": "macOS 用をダウンロード",
      "linux-appimage": "Linux 用をダウンロード",
      "linux-deb": "Linux 用をダウンロード",
    },
    others: "ほかのダウンロード",
    platform: {
      windows: "Windows",
      "macos-arm": "macOS（Apple シリコン）",
      "macos-intel": "macOS（Intel）",
      "linux-appimage": "Linux（AppImage）",
      "linux-deb": "Linux（.deb）",
    },
    version: "バージョン {v}",
    allReleases: "すべてのリリース",
    screenshotAlt: "Nexora の、Fabric のインスタンスのコンテンツタブ（MOD の一覧）",
  },
  compare: {
    title: "ほかのランチャーとの違い",
    lead: "ソースを公開しているランチャーの中で、GPL ではないのは Nexora だけです。コードを自分のプロジェクトに取り込んでも、そのプロジェクトを GPL にする必要はありません。",
    feature: "ランチャー",
    rows: {
      license: "ライセンス",
      source: "ソースコード",
      stack: "作り",
    },
    values: {
      apache: "Apache-2.0",
      gpl3: "GPL-3.0",
      proprietary: "非公開",
      public: "公開",
      closed: "非公開",
      rustTauri: "Rust ＋ Tauri",
      cppQt: "C++ ＋ Qt",
      unknown: "非公開",
    },
    official: "公式ランチャー",
    checked: "2026 年 11 月 23 日に、各プロジェクトの公開リポジトリと公式サイトで確認しました。",
    deps: "Nexora の依存は MIT、Apache-2.0、BSD、ISC などのライセンスです。GPL と LGPL のものはなく、5 つがファイル単位で効く MPL-2.0 です。",
  },
  features: {
    title: "できること",
    items: [
      {
        icon: "sync",
        title: "共通設定",
        body: "インスタンスを共通の設定に結び付けると、キー割り当てなどのゲームの設定がどのインスタンスでも同じになります。",
      },
      {
        icon: "modrinth",
        title: "Modrinth から MOD、​リソースパック、​シェーダー",
        body: "検索して、そのままインスタンスに入れます。必要な MOD も一緒に入り、更新はボタン 1 つです。",
      },
      {
        icon: "import",
        title: "ほかのランチャーから引っ越し",
        body: "Modrinth App、Prism Launcher、公式ランチャー、CurseForge アプリのインスタンスを取り込みます。元のデータは読むだけで変えません。",
      },
      {
        icon: "upgrade",
        title: "Minecraft のバージョンを上げる",
        body: "上げる前に、新しいバージョンに対応していない MOD を知らせます。ワールドは先に退避します。",
      },
      {
        icon: "pack",
        title: ".mrpack の読み込みと書き出し",
        body: "Modrinth のモッドパックを新しいインスタンスとして開き、インスタンスを .mrpack として書き出します。",
      },
      {
        icon: "crash",
        title: "クラッシュの手がかり",
        tag: "試験的",
        body: "ゲームがエラーで終わったとき、ログを読んで原因の候補になる MOD を挙げます。",
      },
    ],
  },
  install: {
    title: "はじめて起動する前に",
    lead: "配布物にはコード署名をしていないため、初回の起動で OS が警告を出します。",
    items: [
      {
        os: "Windows",
        body: "SmartScreen の「Windows によって PC が保護されました」で、「詳細情報」を押してから「実行」を選びます。",
      },
      {
        os: "macOS",
        body: "いちど開いたあと、「システム設定 → プライバシーとセキュリティ」で、Nexora の横の「このまま開く」を押します。",
      },
      {
        os: "Linux",
        body: "AppImage に実行権限を付けて（chmod +x）起動します。自動で更新されるのは AppImage だけで、.deb は新しいものを手で入れ直します。",
      },
    ],
    updates: "Nexora は起動時に新しいバージョンを確認し、更新のボタンを押すまでは何もインストールしません。",
  },
  footer: {
    license: "Apache-2.0",
    disclaimer: "Minecraft は Mojang AB の商標です。Nexora は Mojang、Microsoft とは関係のない非公式のツールです。",
  },
};

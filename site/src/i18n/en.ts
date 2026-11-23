export const en = {
  meta: {
    title: "Nexora Launcher — an open-source Minecraft launcher",
    description:
      "An open-source launcher for Minecraft: Java Edition on Windows, macOS and Linux. Built with Rust and Tauri, licensed under Apache-2.0.",
  },
  nav: { github: "GitHub", language: "日本語", languageHref: "/ja/" },
  hero: {
    title: "A Minecraft launcher you can fork.",
    lead: "Nexora is an open-source launcher for Minecraft: Java Edition, built with Rust and Tauri. It runs on Windows, macOS and Linux, and it is Apache-2.0 with no GPL dependencies.",
    download: "Download",
    downloadFor: {
      windows: "Download for Windows",
      "macos-arm": "Download for macOS",
      "macos-intel": "Download for macOS",
      "linux-appimage": "Download for Linux",
      "linux-deb": "Download for Linux",
    },
    others: "Other downloads",
    platform: {
      windows: "Windows",
      "macos-arm": "macOS (Apple Silicon)",
      "macos-intel": "macOS (Intel)",
      "linux-appimage": "Linux (AppImage)",
      "linux-deb": "Linux (.deb)",
    },
    version: "Version {v}",
    allReleases: "All releases",
    screenshotAlt: "The content tab of a Fabric instance in Nexora, listing its mods",
  },
  compare: {
    title: "How it compares",
    lead: "Among launchers whose source is public, Nexora is the one that is not GPL. You can take its code into your own project without your project having to become GPL.",
    feature: "Launcher",
    rows: {
      license: "License",
      source: "Source code",
      stack: "Built with",
    },
    values: {
      apache: "Apache-2.0",
      gpl3: "GPL-3.0",
      proprietary: "Proprietary",
      public: "Public",
      closed: "Not public",
      rustTauri: "Rust + Tauri",
      cppQt: "C++ + Qt",
      unknown: "Not public",
    },
    official: "Official launcher",
    checked: "Checked against each project's repository and site on November 23, 2026.",
    deps: "Nexora's dependencies are under MIT, Apache-2.0, BSD, ISC and similar licenses. None are GPL or LGPL; five are MPL-2.0, which applies file by file.",
  },
  features: {
    title: "What it does",
    items: [
      {
        icon: "sync",
        title: "Shared settings",
        body: "Link instances to a shared profile so key bindings and other game options stay the same in each of them.",
      },
      {
        icon: "modrinth",
        title: "Mods, resource packs and shaders from Modrinth",
        body: "Search and add them to an instance. Required mods come along, and updates are one button.",
      },
      {
        icon: "import",
        title: "Move from another launcher",
        body: "Bring instances over from the Modrinth App, Prism Launcher, the official launcher or the CurseForge app. The originals are only read.",
      },
      {
        icon: "upgrade",
        title: "Upgrade the Minecraft version",
        body: "Before an instance moves to a newer version, Nexora lists the mods that have no build for it and backs up your worlds.",
      },
      {
        icon: "pack",
        title: ".mrpack in and out",
        body: "Open Modrinth modpacks as new instances, and export an instance as a .mrpack.",
      },
      {
        icon: "crash",
        title: "Crash clues",
        tag: "Experimental",
        body: "When the game exits with an error, Nexora reads the logs and suggests mods that may be the cause.",
      },
    ],
  },
  install: {
    title: "Before the first launch",
    lead: "The downloads are not code-signed, so your system warns you the first time.",
    items: [
      {
        os: "Windows",
        body: "SmartScreen says it protected your PC. Choose More info, then Run anyway.",
      },
      {
        os: "macOS",
        body: "Open the app once, then go to System Settings → Privacy & Security and choose Open Anyway next to Nexora.",
      },
      {
        os: "Linux",
        body: "Make the AppImage executable (chmod +x) and run it. Only the AppImage updates itself; install a new .deb by hand.",
      },
    ],
    updates: "Nexora checks for a new version at startup and installs nothing until you press the update button.",
  },
  footer: {
    license: "Apache-2.0",
    disclaimer: "Minecraft is a trademark of Mojang AB. Nexora is not affiliated with Mojang or Microsoft.",
  },
};

export type Dict = typeof en;

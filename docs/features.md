# Features and status

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

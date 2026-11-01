# Privacy

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

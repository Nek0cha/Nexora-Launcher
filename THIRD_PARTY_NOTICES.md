# Third-party notices

Nexora Launcher is licensed under the Apache License 2.0 (see `LICENSE`).

The licenses of the third-party software bundled in a release are listed in the app, under Settings → About → Third-party licenses. The list is generated from the Rust and frontend dependencies:

```sh
node scripts/gen-licenses.mjs
```

The generated text is committed as `ui/src/assets/third-party-licenses.json`. The release workflow regenerates it before building. The script fails when a dependency uses a license that is not on its allow list (`scripts/licenses-lib.mjs`).

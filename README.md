# OpenAgora

An open-source app store for your desktop. Install open-source apps, use them
side by side in one window, and let one agent work across all of them.

- **One window, real apps.** Each app keeps its own interface, in a tab you can
  switch to, detach into its own window, or split side by side.
- **One agent across everything.** [Hermes](https://github.com/NousResearch/hermes-agent)
  is built in. Ask it to "install T3 Code", check on your threads, or
  remember a decision, and it works across every app you've installed.
- **Open catalog.** Each app is one `app.toml` file. Anyone can add an app by
  pull request.
- **macOS, Linux and Windows** from one codebase.

An *agora* was the open marketplace of a Greek city, and Hermes was the god of
markets and messengers.

## Status

Early. What exists today:

- The listing format and its validator ([`engine/src/manifest.rs`](engine/src/manifest.rs)).
- Four listings: [T3 Code](catalog/apps/t3code/app.toml),
  [Hermes](catalog/apps/hermes/app.toml), [Maccy](catalog/apps/maccy/app.toml)
  (clipboard history) and [Handy](catalog/apps/handy/app.toml) (speech-to-text).
- The `openagora` CLI: `catalog`, `show` and `validate`.

Next up is installing and running apps. The full design and roadmap are in
[docs/SPEC.md](docs/SPEC.md).

## Try the CLI

```sh
cd engine
cargo run -- --catalog ../catalog catalog
cargo run -- --catalog ../catalog show t3code
cargo test
```

## Add an app

1. Create `catalog/apps/<id>/app.toml`; the existing listings are the template.
2. Run `cargo run -- --catalog ../catalog validate` from `engine/`.
3. Open a pull request.

Listings must install from the app's official source and declare what the
agent may do with the app.

## License

MIT. Each app in the catalog has its own license, shown in its listing.

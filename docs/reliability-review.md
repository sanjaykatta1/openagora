# Reliability review and AI-assisted development

Reviewed 15 October 2026 against main `f39cb60` (0.4.0). This is a diagnosis and implementation plan grounded in this repository, not a claim that every supported app or physical desktop has been certified.

## What went wrong

| Finding | Evidence | Change / remaining work |
|---|---|---|
| “Running” did not mean a usable native app | Handy was always launched with `--start-hidden`; Library offered Stop but no way to raise its window. Its upstream single-instance handler explicitly supports raising the window. | Add a manifest-declared native Open action in CLI, desktop and agent gateway. Reopening retains the original process. |
| Linux packaging assumed FUSE existed | The real Handy 0.9.8 AppImage failed on disposable Ubuntu 24.04 and 26.04 with no FUSE device. The complete extracted AppDir displayed a real window on both. | Extract at installation, retain all resources, launch AppRun. No root or FUSE required for installation. |
| Linux availability hid prerequisites | A minimal Ubuntu environment also lacked EGL; upstream requires gtk-layer-shell. Ubuntu 26.04 Wayland dictation requires its own input/shortcut setup. | Show OS-specific prerequisites before installation and in the app page. Keep setup with Handy; do not silently change input permissions. |
| Recovery stopped after the first attempt | `SundayHome.autoSetup` stayed true after a failed setup or a later missing profile; Try again merely refreshed status. | Reset the attempt on explicit retry and after leaving setup. Keep operations busy until state refresh finishes. |
| Hermes could delete files but leave removal incomplete | Real Hermes 0.22.2 removed Sunday’s folder and then failed its routing-identity settlement. A generic failure left recovery stuck. | Recognize the upstream retry instruction, run Hermes’s own `profile purge-identity sunday`, and persist pending cleanup so a failed retry remains recoverable. Never alter its database directly. |
| Cached state outlived the actual installation | A saved profile could report Ready with no Hermes executable; an externally recreated profile retained old connection flags. | Check current ownership markers, clear stale connections on recreation, offer reinstall when managed app files are missing. |
| An old record could claim another profile | Setup/removal treated any saved profile record as ownership, even if the user replaced the directory. | Require the current OpenAgora SOUL marker or explicit adoption. Preserve an unowned replacement. |
| Repair could replace files beneath a running app | Missing installation files did not stop an already tracked process before reinstall cleared its managed directory. | Stop it before repair. Also add coverage for the existing post-install file check; that check was already correct. |
| Tests proved only part of the user journey | Existing Sunday UI tests ended at removal, then restarted with an already-ready fake. Generic Linux CI did not launch the actual Handy release. | Test repeated removal/recreation, external deletion, failed setup retry, native Open/Reopen, and actual Ubuntu window visibility. |
| Integration assumptions changed upstream | PR #4 fixed T3 Code's one-time sign-in URL. PR #14 addresses missing Intel Mac upstream packages; it remains separate. PR #16 restricted desktop Mac support. | Treat artifact architecture, startup addresses and upstream setup as contracts that need explicit verification. Do not equate OS support with CPU support. |
| Release confidence was overstated | Earlier 0.3.1/0.3.2 notarization retries and superseded runs made “ready” ambiguous. | Verify one intended commit, complete assets, signing/notarization, checksums and updater metadata before calling a release ready. A green mock test is insufficient. |

The original checkout also contains substantial uncommitted Porch work. Reliability changes were made in a separate worktree from current main; that work was preserved.

## AI tools worth using

The recommendation is **one coding assistant, one independent reviewer, and deterministic tests that exercise the actual product**. Adding several overlapping review bots would increase noise without proving that Handy opens. Tool capabilities below come from primary documentation; suitability is our assessment, not a benchmark result.

| Tool | Best use here | Limits, cost and data considerations | Decision |
|---|---|---|---|
| [Playwright test agents](https://playwright.dev/docs/test-agents) | Plan lifecycle journeys, generate tests from observed UI, investigate failures with browser evidence. | The repo currently uses `playwright-core` 1.55 and a custom Electron harness. Test-agent integration needs a deliberate upgrade/adaptation. Review healer changes: never accept removed assertions, skipped tests, or mock-only replacements as a product fix. Model access can have a cost. | Highest-value AI testing candidate. Keep the existing Electron harness and real Linux smoke test as the execution authority. |
| [CodeRabbit](https://docs.coderabbit.ai/faq) | Independent PR review with path-specific instructions, especially engine/renderer lifecycle boundaries and missing test cases. | Hosted code access and plan-dependent limits. Review [current OSS eligibility/limits](https://docs.coderabbit.ai/management/plans) and the [privacy policy](https://www.coderabbit.ai/privacy-policy), including OSS-specific terms, before installation. OSS access currently includes Team features without a paid subscription, with separate limits; repositories under ten stars require manual review triggers. | Best low-maintenance reviewer to evaluate for a solo developer. Enable only one reviewer, initially on selected PRs. |
| [GitHub Copilot code review](https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/copilot-code-review) | Review directly in the existing PR workflow; repository instructions can focus on recovery and platform differences. | Review availability/usage depends on the account. Its review is advisory; ensure updated commits receive review. It does not replace installation testing or security analysis. | Good alternative if already included in the developer's account; do not run it and CodeRabbit by default. |
| [PR-Agent](https://github.com/The-PR-Agent/pr-agent) | An open-source, self-hosted reviewer with selectable model providers, including local inference. | MIT/community-maintained; distinct from Qodo's commercial service. Self-hosting still needs maintenance and model compute. Hosted model calls still send code to that provider. Its current README identifies a disabled feature pending a credential-exposure fix: pin and review the chosen release. | Preferred option if open-source tooling/control outweighs convenience. Start with read-only review, not automatic fixes or merges. |
| [Semgrep Multimodal](https://docs.semgrep.dev/semgrep-multimodal/overview) | Explain and triage security findings around shell execution, IPC, downloads and permission boundaries. | AI features require Semgrep AppSec Platform; they are not the same as the community CLI. Language/rule coverage and commercial terms need evaluation. | Later security pilot; do not add another platform just to fix native startup. |
| [Sentry Seer](https://docs.sentry.io/product/ai-in-sentry/seer) | Correlate real crash reports and source to propose causes/fixes after release. | Needs useful telemetry, consent and redaction first. This app handles window contents, agent prompts and upstream account settings: never upload those by default. Review [Seer's data terms](https://sentry.io/trust/) and current account pricing. | Defer until a minimal, opt-in diagnostic design exists. Local startup evidence is more appropriate now. |
| [CodeQL + Copilot Autofix](https://docs.github.com/en/code-security/how-tos/manage-security-alerts/manage-code-scanning-alerts/resolve-alerts) | Deterministic security findings with optional suggested fixes; CodeQL currently lists [Rust and JS/TS](https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/). | One-step Autofix is available for public repositories. Agentic Autofix is a separate feature that consumes Code Security AI credits and Actions minutes. Neither guarantees a correct fix. | Evaluate CodeQL as the security baseline; keep automated security fixes in reviewed PRs. |

No new AI service, subscription, repository secret or telemetry upload was enabled by this change. The implemented workflow improvements work without a paid model service.

## Working loop

1. Record the exact app/release, OS version, architecture, desktop/session type, installation origin and failing action. Use the app-problem issue template; do not paste keys, Hermes `.env`, conversation history or unredacted screen captures.
2. Reproduce before proposing a fix. Separate catalog errors from upstream errors and machine prerequisites. Keep the downloaded release version and startup output.
3. Ask the coding assistant for the smallest behavior change and a regression that would fail beforehand. Existing assistants retain their upstream setup, accounts, channels and profiles.
4. Run unit and lifecycle tests, then a visible desktop journey. Exercise install → open → hide/close → reopen → stop → remove → reinstall; also interrupt/fail setup once and retry.
5. Review the diff independently using the chosen reviewer and the PR template. Require file/line evidence and a realistic failing sequence; dismiss unsupported claims only with an explanation. Never let a reviewer weaken a required check to make CI green.
6. Validate the current commit on each supported platform. For native Linux apps, run the catalog smoke workflow and inspect its evidence. Changes to Wayland interaction still need a real GNOME/Wayland check: Xvfb is X11 and cannot certify shortcuts, microphone access or ydotool permissions.
7. Prepare a release from one verified commit. Keep a draft until CLI and desktop artifacts, signing, notarization, hashes and updater files agree. Cancel superseded work deliberately; do not publish a partially verified release.

## Added checks and how to run them

- `cargo test --manifest-path engine/Cargo.toml`: engine and lifecycle regression suite.
- `npm run typecheck --prefix desktop`, `npm test --prefix desktop`, `npm run test:ui --prefix desktop`: renderer, IPC and visible desktop flows. On Linux run UI tests under Xvfb.
- `.github/workflows/catalog-smoke.yml`: actual upstream Handy installation and visible-window checks in disposable Ubuntu 24.04/26.04 containers, plus real Hermes installation/dashboard/removal/recreation on Ubuntu 26.04. Saves installed asset/version, logs, PID observations and a window capture. Runs with no host home or credentials and no FUSE device.
- `packaging/tests/handy-linux-container.sh` and `packaging/tests/hermes-linux-container.sh`: entry points for reproducing those container checks locally. Mount the repo read-only at `/src`, including a Linux engine built at `engine/target/debug/openagora`, and a writable evidence directory at `/evidence`.

The smoke test deliberately uses the catalog's current upstream release. A new upstream break should fail visibly and preserve evidence; do not automatically retry until green or silently pin a different version. Dependency/network failures need to be distinguished from product failures.

## Still requiring broader validation

A real Ubuntu 26.04 GNOME/Wayland session with a microphone and the user's chosen dictation setup; real Hermes model sign-in and phone/channel pairing; each remaining catalog app's actual upstream release; Handy’s self-updater in an extracted AppDir (use store removal/reinstallation to refresh its program files); and release signing/notarization on the final release commit. These cannot be certified by a headless container or fake Hermes fixture. The current work does not merge the separate Intel Mac or ArtCraft proposals.

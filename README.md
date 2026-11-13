<!-- markdownlint-disable MD033 MD041 -->
<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/wordmark-dark.svg">
    <img alt="omfp" src="docs/brand/wordmark-light.svg" height="64">
  </picture>
</h1>

<sub>The f stands for fabulous, right?</sub>

**The agent multiplexer for hackers who are tired of prepackaged workflows.**

A browser and desktop app for the [oh-my-pi](https://github.com/can1357/oh-my-pi) coding agent: many
conversations with the agent at once, each with its own oh-my-pi running in the background.

- **Run as many agents as you like.** Every conversation (a thread) is a separate oh-my-pi, and it keeps working when
  you close the window. Pick them up from your laptop or your phone.
- **Bring your subscription.** Works with your ChatGPT or Claude plan, or any API key oh-my-pi supports.
- **Workspaces that stay.** A workspace is a folder holding the repos and worktrees you work on. Threads in the same
  workspace share it, and it is still there tomorrow.
- **The agent can rewrite the UI.** Everything around the chat is made of small React panels. Ask for a test
  dashboard or a deploy button, or let the agent suggest one: it writes the panel and it shows up live. The panels
  omfp ships with work the same way, so you can change those too.
- **Almost no opinions.** A small core, a keyboard-driven shell, and the rest is panels you can copy, replace or
  throw away.
- **Emacs philosophy at heart.** You change the tool from inside it, while it runs. No rebuilds.

Early and changing fast.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/screenshot-dark.png">
  <img alt="omfp with a running thread" src="docs/brand/screenshot-light.png">
</picture>

## Install

**With Nix** (Linux, macOS on Apple silicon): `nix profile install github:aciceri/omfp` (or `nix run
github:aciceri/omfp -- desktop`). It brings the right oh-my-pi with it.

**Without Nix**: download from the [releases](https://github.com/aciceri/omfp/releases); each one comes with its own
oh-my-pi, and you need git.

- **macOS (Apple silicon, macOS 14+)**: open `omfp-<version>-macos-arm64.dmg` and drag omfp to Applications. Unless a
  release says it is notarized, macOS refuses to open it the first time: open it once, then allow it in System
  Settings → Privacy & Security → Open Anyway (or run `xattr -dr com.apple.quarantine /Applications/omfp.app`).
- **Linux desktop (x86_64, Flatpak)**: `flatpak install --user omfp-<version>-linux-x86_64.flatpak`, then start omfp
  from your app menu or with `flatpak run io.github.aciceri.omfp desktop`. The window lives in the sandbox, but
  oh-my-pi, git and the commands of jobs run on your machine on purpose (they need your tools and repositories), so
  the app has full access to your account. Keep workspaces and repositories in your home directory. On NixOS the
  bundled oh-my-pi cannot run: set `omp_bin` in `~/.config/omfp/config.toml`, or use Nix.
- **Linux server (x86_64, glibc 2.34+)**: unpack `omfp-<version>-linux-x86_64.tar.gz` and run `./omfp daemon`, then
  open the URL it prints (the browser UI, no desktop window).

To check a download, compare `shasum -a 256 <file>` (macOS) or `sha256sum <file>` (Linux) with its line in the
release's `SHA256SUMS`.

<sub>This README is for humans. The slop lives in [docs/overview.md](docs/overview.md); anything else, ask your agent
how omfp works.\*</sub>

<sub>\* OK, this one is AI-generated too, but at least I read it.</sub>

# RumuRule — project status

Last updated: 2026-11-12. Scope: Linux-first Xray desktop client (proxy and TUN
modes). macOS is intentionally out of scope.

> Snapshot of what works today and what is still missing. Percentages are
> estimates, not a substitute for the milestone list below.

## Overall

| Scope | Done | Meaning |
| --- | ---: | --- |
| Linux client (proxy + TUN) | **≈98%** | Daily-usable and packaged: import, connect, system proxy, ping, UI, tray, system-wide TUN with a fuse |
| Full roadmap | **≈72%** | Adds the AUR upload, zapret and Windows |

The remaining gap is the AUR upload, the Windows port and zapret; the Linux
client installs from a package and TUN works end to end on a live host.

## Modules

| Area | Status | Done | Notes |
| --- | --- | ---: | --- |
| Core architecture and build | Complete | 100% | Rust workspace; `rumurule-core` has no Tauri dependency; `cargo test` is green |
| Share-link import | Complete | 100% | `vless://`, `vmess://`, `trojan://`, `ss://`, `hysteria2://`/`hy2://` (QUIC, salamander, cert pin) |
| Subscriptions and HWID | Mostly | 95% | Link lists (plain/base64), JSON configs, `profile-title` groups, panel-error detection, stable device id |
| Storage and server list | Complete | 100% | Atomic `profiles.json`, groups, collapse, rename, pin, refresh; pinned → starred → country order |
| Xray sidecar and connection | Complete | 95% | Config generation, `xray run -test` validation before start, lifecycle, log streaming, orphan cleanup, proxy and TUN modes |
| System proxy | Mostly | 95% | KDE + GNOME, snapshot and restore after crash; no Windows/WPAD |
| Latency probes | Mostly | 95% | Proxy GET, TCP, ICMP; `default` / `double` / `keep-alive`; custom test URL |
| Status tab | Complete | 100% | Connect/Disconnect, current server, live session timer |
| Settings UI | Complete | 100% | HWID, tunnel mode and TUN status, system proxy, ping options, core version |
| Tray icon | Works | 95% | Session state in the menu, Open/Disconnect/Quit; closing the window destroys the webview and frees its memory, the tray rebuilds it with the same size, section, server and scroll position; no icon variants per state |
| Material You / KDE theme | Mostly | 95% | Accent-derived palette, live light/dark switch from `kdeglobals` |
| Adaptive layout | Mostly | 90% | Down to 360×480; server rows and the dock reflow at phone widths; desktop window only, no mobile target |
| Motion and animations | Partial | 70% | Low-FPS fallback works; one-frame flicker in menu/slide still open |
| TUN mode and privileged helper | Works | 95% | `rumurule-helper` + polkit action; a one-time welcome setup installs the helper and a per-user rule, so connects do not prompt; interface, routes, DNS takeover, traffic watchdog, session fuse; helper and core ship inside the package |
| Zapret tab and system tunneling | Not started | 0% | Deliberate backlog, separate tab |
| Packaging and AUR | Mostly | 85% | `packaging/` holds the PKGBUILD, desktop entry, icons and polkit action; the package ships a pinned Xray core and installs cleanly with `pacman -U`; AUR upload pending |
| Windows support | Not started | 0% | Linux code paths only |
| Documentation and licensing | Complete | 100% | README with a Russian translation, `docs/tun.md`, screenshots and a recording in `examples/`, AGPL-3.0 `LICENSE`, `NOTICE` |
| Automated tests | Mostly | 85% | 47 tests (42 core, 5 app); no UI or TUN integration tests |

## What is already proven live

- Real subscription imported and grouped by title.
- Traffic passes through Xray with the local SOCKS5/HTTP endpoints.
- KDE and GNOME system proxy applied and restored, including after a kill.
- TCP, ICMP and proxy latency measured on real servers.
- TUN mode: `rumurule0` up, default IPv4/IPv6 routes, `resolvectl` pointed at the
  tunnel, traffic egressing from the server address, DNS and ICMP working, clean
  teardown (routes and resolver restored).
- TUN and the local SOCKS5/HTTP endpoints at the same time; the system proxy can
  stay on in TUN mode.
- TUN with a server given by name: the GUI resolves the domain before the
  tunnel starts and keeps it as the TLS/REALITY server name; egress confirmed
  through an external address check.
- Session fuse: a TUN session whose server cannot pass traffic shuts itself down
  after ~15 s, removes the interface and gives the machine its internet back; a
  session that dies later is reported in the window and in the tray.
- A local Hysteria 2 server (`hysteria` 2.13.0, self-signed cert, salamander
  obfs) reached through Xray 26.11.9 over QUIC; a real subscription Hysteria
  node held a full TUN session with a clean teardown. Xray 26 dropped
  `allowInsecure`, so a self-signed endpoint needs a `pinSHA256` from the link;
  `insecure=1` alone cannot be expressed.
- Status-area icon via libayatana-appindicator, registered by KDE.
- Packaged as `rumurule-0.9.1-1-x86_64.pkg.tar.zst` (17 MB compressed, 45 MB
  installed) and installed on a clean Arch host with a plain `sudo pacman -U`;
  `pacman -Qo` reports the binary, the helper and the bundled core.
- Memory in the release build (PSS): 277 MB with the window open, 139 MB after
  it is closed and the web process is stopped; the tray brings it back.
- Layout checked at 360 and 412 px wide: server rows stay on one line and the
  dock stays inside the window.
- KDE accent and light/dark scheme followed at runtime.

## Known issues and gaps

1. **TUN rough edges.** There is no hard kill switch that blocks plain traffic
   when the tunnel dies; instead the session stops and the machine falls back to
   the physical link. A session that dies on its own is now cleaned up: the
   helper watches the interface, Xray and the GUI process, checks that traffic
   flows, and the GUI restores the system proxy when it detects the end.
2. **Animation flicker.** The menu/slide animation briefly disappears and
   replays for one frame. Low-FPS fallback is in place, root cause not fixed.
3. **AUR is not published.** The PKGBUILD exists and the package works, but
   its source points at a `v0.9.x` tag that is not published and the tarball
   checksum is `SKIP`, so the recipe is only good for a local build.
4. **Proxy ping is only as good as the test URL.** A server that completes a TCP
   handshake but drops traffic reports `fail`, which is correct but noisy.
5. **No CI.** Builds and tests are run manually.
6. **Windows is not started.** Porting system-proxy and TUN integration is a
   separate milestone.
7. **No TUN settings yet.** MTU, gateway range, IPv6 and DNS takeover are fixed
   in `config::tun_config`; no UI to change them.

## Next milestones

| Order | Milestone | Adds |
| ---: | --- | --- |
| 1 | 0.9.2 release (version bump, package rebuild, AUR upload) | Public install |
| 2 | Subscription metadata (user-agent presets, device headers, refresh jitter) | Panels see a familiar client |
| 3 | TUN polish (kill switch, MTU/DNS settings, reconnect on network change) | Production-grade TUN |
| 4 | Animation fix | Remove the one-frame flicker |
| 5 | Windows port | System proxy + TUN for Windows |
| 6 | Zapret tab | DPI bypass and system tunneling (backlog) |

## Project tree

```text
RumuRule/
├── crates
│   └── core
│       ├── examples
│       │   ├── dump_config.rs
│       │   ├── dump_tun.rs
│       │   └── fetch_sub.rs
│       ├── src
│       │   ├── config
│       │   │   ├── mod.rs
│       │   │   ├── outbound.rs
│       │   │   ├── routing.rs
│       │   │   └── stream.rs
│       │   ├── link
│       │   │   ├── mod.rs
│       │   │   ├── shadowsocks.rs
│       │   │   ├── trojan.rs
│       │   │   ├── vless.rs
│       │   │   └── vmess.rs
│       │   ├── store
│       │   │   ├── mod.rs
│       │   │   ├── profiles.rs
│       │   │   └── subscriptions.rs
│       │   ├── subscription
│       │   │   ├── fetch.rs
│       │   │   ├── mod.rs
│       │   │   └── parse.rs
│       │   ├── lib.rs
│       │   ├── model.rs
│       │   ├── proxy.rs
│       │   └── settings.rs
│       └── Cargo.toml
├── docs
│   ├── backlog.md
│   └── tun.md
├── examples
│   ├── icon.png
│   ├── privacy.png
│   ├── session.gif
│   ├── session.mp4
│   └── status.png
├── helper
│   ├── src
│   │   └── main.rs
│   └── Cargo.toml
├── packaging
│   ├── icons
│   │   └── hicolor
│   │       ├── 128x128/apps/slop.rumurule.png
│   │       ├── 16x16/apps/slop.rumurule.png
│   │       ├── 24x24/apps/slop.rumurule.png
│   │       ├── 256x256/apps/slop.rumurule.png
│   │       ├── 32x32/apps/slop.rumurule.png
│   │       ├── 48x48/apps/slop.rumurule.png
│   │       ├── 512x512/apps/slop.rumurule.png
│   │       └── 64x64/apps/slop.rumurule.png
│   ├── PKGBUILD
│   ├── README.md
│   ├── slop.rumurule.desktop
│   └── slop.rumurule.tunnel.policy
├── scripts
│   ├── fetch-xray.sh
│   ├── install-helper.sh
│   └── tun-test.sh
├── src-tauri
│   ├── capabilities
│   │   └── default.json
│   ├── icons
│   │   ├── 128x128.png
│   │   ├── 128x128@2x.png
│   │   ├── 32x32.png
│   │   ├── icon.ico
│   │   └── icon.png
│   ├── src
│   │   ├── commands
│   │   │   ├── mod.rs
│   │   │   ├── ping.rs
│   │   │   ├── profiles.rs
│   │   │   ├── settings.rs
│   │   │   └── tunnel.rs
│   │   ├── system_proxy
│   │   │   ├── gnome.rs
│   │   │   ├── kde.rs
│   │   │   └── mod.rs
│   │   ├── theme
│   │   │   ├── kdeglobals.rs
│   │   │   └── mod.rs
│   │   ├── tun
│   │   │   └── mod.rs
│   │   ├── xray
│   │   │   ├── binary.rs
│   │   │   ├── mod.rs
│   │   │   └── output.rs
│   │   ├── main.rs
│   │   └── window.rs
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
├── ui
│   ├── css
│   │   ├── base.css
│   │   ├── components.css
│   │   ├── dock.css
│   │   ├── layout.css
│   │   ├── responsive.css
│   │   └── theme.css
│   ├── js
│   │   ├── api.js
│   │   ├── log.js
│   │   ├── main.js
│   │   ├── menu.js
│   │   ├── motion.js
│   │   ├── render.js
│   │   ├── settings.js
│   │   ├── tabs.js
│   │   ├── theme.js
│   │   └── welcome.js
│   ├── icon.png
│   └── index.html
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── LICENSE
├── NOTICE
├── README.md
├── README_ru.md
└── STATUS.md
```

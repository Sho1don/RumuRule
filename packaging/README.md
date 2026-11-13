# Packaging

Files here are installed by the `rumurule` package; the development flow uses
`scripts/` instead.

## Installed layout

| Path | What it is |
| --- | --- |
| `/usr/bin/rumurule` | The Tauri app. The UI is embedded at compile time. |
| `/usr/lib/rumurule/rumurule-helper` | Privileged helper, started through polkit. |
| `/usr/lib/rumurule/xray` | Pinned Xray core. |
| `/usr/share/polkit-1/actions/slop.rumurule.tunnel.policy` | The one action the helper may run under. |
| `/usr/share/applications/slop.rumurule.desktop` | Menu entry, `StartupWMClass=rumurule`. |
| `/usr/share/icons/hicolor/*/apps/slop.rumurule.png` | Icon set, generated from `src-tauri/icons/icon.png`. |
| `/usr/share/licenses/rumurule/` | `LICENSE` and `NOTICE`. |

User data stays in `$XDG_CONFIG_HOME/slop.rumurule` (`profiles.json`,
`settings.json`) and `$XDG_DATA_HOME/slop.rumurule` (a core downloaded by
`scripts/fetch-xray.sh`). Packaging never touches either, so installing or
updating the package keeps servers, subscriptions and the device id.

## Core resolution

The app looks for Xray in this order:

1. `RUMURULE_XRAY` if set;
2. `$XDG_DATA_HOME/slop.rumurule/bin/xray` (what `scripts/fetch-xray.sh` installs);
3. `/usr/lib/rumurule/xray` (what the package installs);
4. `xray` on `PATH`.

A user-updated core therefore wins over the packaged one, and a system `xray`
package is still usable through `RUMURULE_XRAY=/usr/bin/xray`.

## Building the package

```sh
cd packaging
makepkg -si
```

The PKGBUILD clones the tag named by `pkgver` and downloads the pinned Xray
archive, whose checksum it verifies. It is only valid once the matching tag
exists; before that, build from a checkout with `cargo build --release`.

The package opts out of makepkg's LTO for a reason noted in `PKGBUILD`; do not
turn it back on without checking that the link still succeeds.

## Notes

- `slop.rumurule` is a two-part application id. AppStream wants a reverse-DNS
  id of at least three parts, so no `metainfo.xml` ships yet; a software-center
  listing would need the id to change, and `sho1don.slop.RumuRule` is the
  intended replacement when that happens. Changing it moves the config
  directories, so it waits for a release that needs it (Flatpak or a distro
  repository) and comes with a migration of `~/.config/slop.rumurule`.
- The helper's own "set up without password" step writes
  `/etc/polkit-1/rules.d/50-rumurule.rules` for the current uid. It rewrites the
  helper and policy that the package already installed with identical content,
  so a package update never fights with it.

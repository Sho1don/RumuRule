# Backlog

Ideas beyond the current Linux proxy-mode milestone. Nothing here is a
commitment; order and scope can change.

## TUN and routing

- **Done: TUN mode + privileged helper.** Built-in Xray TUN, `rumurule-helper`
  started through `pkexec`, one polkit approval for `slop.rumurule.tunnel`,
  routes, DNS takeover and a watchdog. Remaining work is packaging.
- **Done: bypass the proxy server IP.** `autoOutboundsInterface: "auto"` binds
  Xray sockets to the physical interface, so traffic to the server never loops
  back into the tunnel. Loopback and private ranges stay on the direct outbound.
- **Local proxy connection.** Expose the local SOCKS/HTTP endpoints next to TUN,
  so selected apps can bypass the tunnel without disconnecting.
- **Kill switch.** Block traffic outside the tunnel while it is running, and
  decide what happens when Xray dies.
- **TUN settings.** MTU, tunnel subnet, IPv4/IPv6 routing and DNS takeover
  should be configurable instead of fixed in `config::tun_config`.
- **Recover after network changes.** Wi-Fi roam, suspend and a new default route
  currently need a reconnect; the helper should rebuild routes on its own.

## Zapret

- **Separate Zapret tab.** Run DPI-bypass rules and system-wide tunneling from
  its own section. Needs a clear privilege model and a way to restore the
  firewall/routing state after a crash.

## Mobile interface

- Desktop layout is stable; phone-sized windows still need work:
  compact Settings, safe-area handling, dock spacing, 20:9 and 19.5:9 aspect
  ratios, larger touch targets, and no clipped fields near the bottom dock.
- Fix the remaining one-frame animation flicker (menu and view slide).

## More protocols

Current state:

| Protocol | Share links | Notes |
| --- | --- | --- |
| VLESS + REALITY | Yes | Including TLS, WS, gRPC and `xtls-rprx-vision` |
| Trojan (TLS) | Yes | |
| Shadowsocks (2022-blake3) | Yes | Method is passed through to Xray; needs a live 2022 test |
| VMess (TLS) | Yes | |
| Hysteria 2 | JSON subscriptions only | No `hysteria2://` parser yet; decide between Xray and a Hysteria sidecar |
| TUIC | No | Needs a separate core or a different Xray build |
| NaiveProxy | No | Needs the Naive core |
| AmneziaWG | No | WireGuard-based; different data path from Xray |
| AdGuard TrustTunnel | No | Research protocol and client support first |

Research order: **Mieru** first
(<https://github.com/enfein/mieru>), then Hysteria 2, TUIC and NaiveProxy.

### Mieru and the GPL

Mieru is GPL-3.0. RumuRule is AGPL-3.0; GPLv3 and AGPLv3 are compatible for
combined works, but the simplest and safest route is to keep Mieru as a
**separate sidecar process** started by RumuRule. In that case:

- do not copy Mieru code into RumuRule;
- ship Mieru's license text and a written offer for its source;
- keep our protocol glue original.

Reimplementing a protocol from its specification is fine; copying GPL code
without following its terms is not.

## Licensing

- Project relicensed from MIT to AGPL-3.0-only on 2026-11-10. Previously
  published MIT versions stay MIT.

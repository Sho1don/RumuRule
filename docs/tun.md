# TUN mode internals

This is the detail behind the short TUN section of the README. It is written for
someone reading the code, not for a first run.

## Ownership

- The helper validates the configuration, copies it to `/run/rumurule/` with
  root-only permissions and starts Xray. It watches the GUI process and removes
  the interface, routes and resolver entry when Xray exits.
- The GUI stops the tunnel by writing a file in `$XDG_RUNTIME_DIR/rumurule-<uid>/`,
  because it cannot signal a root process. A stale tunnel is torn down when its
  GUI process disappears.
- `autoOutboundsInterface: "auto"` binds Xray's own sockets to the physical
  interface, so traffic to the server never re-enters the tunnel. That replaces
  hand-made host routes for the server address.

## Traffic probe

The helper checks that the tunnel actually carries traffic: an HTTP request
through the tunnel's own SOCKS port before the session is announced, then every
15 s. Three failures in a row take the tunnel down, so a broken server or a
changed network never leaves the machine without internet. Routes, DNS and the
desktop proxy are restored on every exit path.

A session that dies on its own (crashed Xray, removed link, dropped network) is
picked up by a background supervisor, which restores the system proxy, writes it
to the log, updates the tray and shows a notice in the window instead of leaving
the desktop pointed at a dead port.

## DNS and routing

- FakeDNS and loopback resolvers are dropped from subscription configs in TUN
  mode: fake addresses cannot be routed, and a local resolver would loop back
  into the tunnel. Sniffing still recovers domain names for routing.
- Outbound server domains are resolved before the tunnel starts and stored as
  addresses, with the domain kept as the TLS/REALITY server name. Inside the
  tunnel DNS answers come from the tunnel itself, so a domain outbound could
  never resolve its own server.
- The interface MTU is 1400: a tunneled packet must still fit the path MTU or
  large TLS handshakes stall.

## Debugging

`scripts/tun-test.sh <profiles.json> <index> [seconds]` starts a session from the
command line with a dead-man's switch: the helper stops when the script exits,
after the time limit, or when a detached watchdog touches the stop file. It is
the safe way to debug TUN without locking yourself out.

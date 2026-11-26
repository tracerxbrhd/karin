<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

# ADR-0038: Local Discord Rich Presence

Status: Accepted for the 0.8.3 candidate.

The presence preference starts off. When enabled, the desktop adapter projects
actual player events into one latest-value mailbox and one local IPC worker.
Track, source, seek and transport changes update the activity. Position ticks
do not generate packets. Playback clocks use the RPC example's Unix seconds.
Paused activities omit timestamps. A stop clears the activity, and logout,
disable and close retire the connection. The worker checks the current
mailbox identity immediately before an activity write. An admitted exchange
can still be in flight for up to 800 ms before it is cleared or disconnected.

Discord's documented IPC handshake uses Karin's public application ID.
The worker bounds input, connection retries, exchanges and activity writes.
It runs without an OAuth token, credentials, network listeners or new
dependencies. The Rust `discord-rich-presence` library has a compatible
activity envelope. Direct implementation keeps the shared transport limited
to the precise operations Karin needs and uses the existing Rustix and Serde
dependencies. [Discord RPC](https://discord.com/developers/docs/topics/rpc)
· [Rust activity types](https://github.com/vionya/discord-rich-presence/blob/master/src/activity.rs)

An eligible track has available catalog metadata, a canonical track ID and a
canonical album relationship. Discord receives the display title, artist,
album when known, timing and canonical public links. External art is limited
to the validated public Music content CDN. A provision exists for registering
`karin_logo` later. Discord documents Listening, a progress bar with both
timestamps, publicly hosted HTTPS artwork and two button objects. Buttons
are visible to other users. Local fixture tests validate the outgoing schema,
confirmation and connection lifecycle. Desktop rendering remains an
acceptance check. [Activity fields](https://discord.com/developers/docs/events/gateway-events#activity-object)
· [Discord Rich Presence guide](https://discord.com/developers/docs/discord-social-sdk/development-guides/setting-rich-presence)

Native probes the documented socket names and the stock Flathub Discord
export. Flatpak grants read-only access to the native primary socket and
Flathub Discord's dedicated runtime directory. This grants no session bus
names. Direct use of the exported directory allows socket replacement while
its mount is live. A native socket mounted as a file can go stale when
recreated, and a path absent at sandbox startup is not mounted. Start Discord
and relaunch Karin when a Flatpak mount is missing or stale. New permissions
require a system update from published 0.8.2 under the Portal contract.
[Flathub Discord launcher](https://github.com/flathub/com.discordapp.Discord/blob/master/discord.sh)
· [Flatpak filesystem setup](https://github.com/flatpak/flatpak/blob/main/common/flatpak-context.c)
· [Flatpak Portal contract](https://github.com/flatpak/flatpak/blob/main/data/org.freedesktop.portal.Flatpak.xml)

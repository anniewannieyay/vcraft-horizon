# Signing in to a VCraft relay

How a VCraft client proves to the relay that it owns its Minecraft account,
without the relay being able to borrow that proof for a real server.

## The problem it fixes

Earlier versions took the Mojang session `serverId` from the relay's
`challenge` and passed it straight to `joinServer`. A malicious or compromised
relay could send the hash of a handshake it was doing with a real online-mode
server; that server would then accept the relay as the player.

## The protocol (from VCraft 2.5.1)

1. The relay sends `{"t": "challenge", "nonce": "<nonce>"}`, with `"offline": true`
   when it doesn't check accounts. The nonce is fresh for every connection:
   16 to 128 characters of `[A-Za-z0-9_-]` (for example 32 random bytes in
   base64url).
2. The client computes the session id itself:

   ```
   serverId = minecraftHexDigest(SHA-1("vcraft-relay-v1|" + host + "|" + nonce))
   ```

   - `host` is the relay URL's host name in lowercase, without scheme, port or
     path (`vcraft.2-31-104-109.sslip.io`).
   - `minecraftHexDigest` is Java's `new BigInteger(digest).toString(16)`: signed,
     lowercase, no leading zeros (the format Minecraft uses for server hashes).
3. The client calls Mojang's `joinServer` with that `serverId`, then sends `login`.
4. The relay computes the same `serverId` from its own host name and the nonce
   it sent, and calls `hasJoined(username, serverId)`.

A relay can't choose a nonce that makes this equal a real server's hash
(that would need a SHA-1 preimage), and the host name ties the proof to one
relay.

A client never falls back to a `serverId` sent by the relay: if the challenge
has no valid `nonce` (and isn't `offline`), it stops and says the relay needs
updating.

## Compatibility

- The relay can keep sending a `serverId` field for older clients while they
  update; set it to the derived value above, so old clients join with a value
  the relay didn't choose freely.
- The Fabric mod needs the same client change, or a hostile relay can still
  abuse Fabric players' sessions.

Test values (`minecraftHexDigest(SHA-1(text))`):

| Text | Digest |
| --- | --- |
| `Notch` | `4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48` |
| `jeb_` | `-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1` |
| `simon` | `88e16a1019277b15d58faf0541e11910eb756f6` |

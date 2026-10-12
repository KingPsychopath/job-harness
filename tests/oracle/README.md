# Frozen Python compatibility oracle

The 0.1 implementation is retained only for cross-language migration checks. Rust never imports it and native installation/release archives exclude it. Source paths were adapted to the one canonical generic resources directory; domain behavior is frozen.

Run `make check-parity` from the repository root. That checks versioned envelopes, existing packets and idempotency, unknown blocking, journal extension fields, exact pipeline views, transfers in both directions and cross-language file locking. Golden vectors in the Rust tests cover historical codecs and identity functions. The previous test suite and wheel build remain in Git history.

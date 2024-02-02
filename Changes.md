## 0.2.6

- Removed unneeded Arc<> around reqwest client
- Supports graceful shutdown
- Upgraded axum and other dependencies

## 0.2.5

Something broke for netbsd, so I'm removing it from the supported platforms.

## 0.2.4

Added --write-pid and an example freebsd rc.d script.

## 0.2.3

Added /refresh endpoint to force a refresh of the cache.

## 0.2.2

Remove unsupported platforms

## 0.2.1

Upgrade some dependencies (including ring) to try to fix some build failures.

## 0.2.0

Add support for generating a manpage, new build tooling (stolen from drolsky's
precious tool), and a `--once` flag.

## 0.1.0
First release

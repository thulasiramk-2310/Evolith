# DevReplay

Git tells you what changed. DevReplay tells you how it happened.

DevReplay is an open-source feature reconstruction engine that records development activity, reconstructs sessions and features, and enables replay of how software was built.

Events → Sessions → Features → Stories

## Milestone 0

Currently supports:
* Event storage
* Session reconstruction
* Replay

## Usage

```bash
cargo build --release
./target/release/devreplay init
./target/release/devreplay import samples/sample-events.json
./target/release/devreplay events list
./target/release/devreplay sessions rebuild
./target/release/devreplay replay session <id>
```

#!/bin/sh
echo "Starting Halo for Capture..."
#cargo run --release -- --source-ip 127.0.0.1 --lighting-dest-ip 127.0.0.1 --pixel-dest-ip 127.0.0.1 --show-file shows/Guys40th.json
cargo run -- --source-ip 127.0.0.1 --lighting-dest-ip 127.0.0.1 --pixel-dest-ip 127.0.0.1 --broadcast --show-file shows/Jasons40th.json

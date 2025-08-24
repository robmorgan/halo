#!/bin/sh
echo "Starting Halo..."
#cargo run -- --source-ip 10.143.62.113 --show-file shows/Guys40th.json
#cargo run -- --source-ip 127.0.0.1 --dest-ip 10.8.45.80 --show-file shows/Guys40th.json
#cargo run -- --source-ip 127.0.0.1 --dest-ip 10.8.45.80 --show-file shows/Guys40th.json
#

# doesn't work when capture is running so use this (for broadcast):
#cargo run -- --source-ip 10.8.45.1 --show-file shows/Guys40th.json

# unicast - using local IP address
#cargo run -- --source-ip 192.168.1.131 --dest-ip 192.168.1.131 --show-file shows/Guys40th.json

#cargo run -- --source-ip 192.168.1.131 --dest-ip 192.168.1.131 --show-file shows/Guys40th.json
cargo run -- --source-ip 10.8.45.1 --dest-ip 10.8.45.80 --show-file shows/Guys40th.json

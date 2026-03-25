# WRTS: World of Warships Real-Time Strategy Game
This game is a project I made for me and my friends.

Inspired by WoWS, you control a team of ships from ww2 and win by having the last ship standing

## Running:
1. Install Rust
2. Install [cargo-make](https://github.com/sagiegurari/cargo-make) by running `cargo install --force cargo-make`
3. Clone the repository to your local system
4. In the root directory of this repository, run `cargo make client` to start the client and `cargo make server` to start the server.
These should be done in separate terminal windows.
5. Once the server is running, connect to it from a client by typing in the server's IP address (the default port is 4433, set in wrts_messaging)
6. Once two clients are connected, the game will automatically start

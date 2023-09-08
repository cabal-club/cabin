# cabin

A Rust(ic) Cabal client.

Cabin is a command-line Cabal client written in Rust. It uses the [cable.rs](https://github.com/cabal-club/cable.rs) implementation of the new [Cable protocol](https://github.com/cabal-club/cable).

**Status**: alpha (under active construction; expect changes).

## Usage

Begin by cloning, building and running `cabin`:

```
git clone git@github.com:cabal-club/cabin.git
cd cabin
cargo build --release
./target/release/cabin
```

### Add a Cabal

Once `cabin` has launched, a cabal must be added from the `!status` window. For example:

`/cabal add 1115a517c5922baa9594f5555c16e091ce4251579818fb4c4f301804c847f222`

Having at least one active cabal is a prerequisite for many other behaviours and actions of `cabin`. Multiple cabals are active for each instance of `cabin`.

### Listen for TCP Connections

`cabin` uses TCP to make connections with peers. Start a TCP listener by providing a port and, optionally, an IP (a default IP of `0.0.0.0` is used when an IP is not explicitly provided):

`/listen 8007`

### Connect to a Peer Over TCP

Once you know the IP / hostname and port of a listening `cabin` instance, a connection can be attempted as follows:

`/connect 25.1.204.77:8007`

### Join a Channel

Channels can be joined using the `/join` / `/j` commands:

`/join myco`

## Help

From the `!status` window, type `/help` and press `<ENTER>` to display the help menu. If the active window is a channel and you wish to return to the `!status` window, type `/win 0`.

```
[17:58] -status- /help
[17:58] -status- /cabal add ADDR
[17:58] -status-   add a cabal
[17:58] -status- /cabal set ADDR
[17:58] -status-   set the active cabal
[17:58] -status- /cabal list
[17:58] -status-   list all known cabals
[17:58] -status- /channels
[17:58] -status-   list all known channels
[17:58] -status- /connections
[17:58] -status-   list all known network connections
[17:58] -status- /connect HOST:PORT
[17:58] -status-   connect to a peer over tcp
[17:58] -status- /delete nick
[17:58] -status-   delete the most recent nick
[17:58] -status- /join CHANNEL
[17:58] -status-   join a channel (shorthand: /j CHANNEL)
[17:58] -status- /listen PORT
[17:58] -status-   listen for incoming tcp connections on 0.0.0.0
[17:58] -status- /listen HOST:PORT
[17:58] -status-   listen for incoming tcp connections
[17:58] -status- /members CHANNEL
[17:58] -status-   list all known members of the channel
[17:58] -status- /topic
[17:58] -status-   list the topic of the active channel
[17:58] -status- /topic TOPIC
[17:58] -status-   set the topic of the active channel
[17:58] -status- /whoami
[17:58] -status-   list the local public key as a hex string
[17:58] -status- /win INDEX
[17:58] -status-   change the active window (shorthand: /w INDEX)
[17:58] -status- /exit
[17:58] -status-   exit the cabal process
[17:58] -status- /quit
[17:58] -status-   exit the cabal process (shorthand: /q)
```

## Logging

Logging of various levels can be enabled by specifying the `RUST_LOG` environment variable:

`RUST_LOG=debug ./target/release/cabin`

It can be helpful to redirect the `stderr` of the process to a file when debugging:

`RUST_LOG=debug ./target/release/cabin 2> output`

Or to a terminal:

First run the command `tty` in an empty terminal. It should return a file path. For example:

```
tty
/dev/pts/2
```

The path can then be used to direct `stderr` to the empty terminal:

`RUST_LOG=debug ./target/release/cabin 2> /dev/pts/2`

## Developer / Contributor Guide

Wherever possible, idiomatic Rust conventions have been followed regarding code formatting and style. Doc and code comments can be found throughout the codebase and will guide you in any contribution efforts. In addition, there are examples and tests to read and learn from. With all that being said, there is still much room for improvement and contributions are welcome.

Before beginning work on any contributions, please open an issue introducing what you wish to work on. A project maintainer will respond and help to ensure that the intended contribution is a good fit for the project and that you are supported in your efforts. The issue can later be referenced in any subsequent pull-requests.

When it comes to code styling, it's recommended to refer to the codebase and follow the established stylistic conventions. This is not a hard requirement but a consistent codebase helps to facilitate clarity and ease of understanding. When in doubt, open an issue to ask for guidance.

There are many code comments throughout the codebase labelled with `TODO`; these may provide some inspiration for initial contributions.

## Contact

glyph (glyph@mycelial.technology).
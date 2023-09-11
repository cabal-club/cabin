use std::{
    collections::{HashMap, HashSet},
    io::Read,
};

use async_std::{
    net,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use cable::{error::Error, post::PostBody, Channel, ChannelOptions};
use cable_core::{CableManager, Store};
use futures::{channel::mpsc, future::AbortHandle, stream::Abortable, SinkExt};
use log::{debug, error};
use terminal_keycode::KeyCode;

use crate::{
    hex,
    input::InputEvent,
    time,
    ui::{Addr, TermSize, Ui},
};

type StorageFn<S> = Box<dyn Fn(&str) -> Box<S>>;

type CloseChannelSender = mpsc::UnboundedSender<Channel>;
type CloseChannelReceiver = mpsc::UnboundedReceiver<Channel>;

/// A TCP connection and associated address (host:post).
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum Connection {
    Connected(String),
    Listening(String),
}

pub struct App<S: Store> {
    abort_handles: Arc<Mutex<HashMap<Channel, AbortHandle>>>,
    cables: HashMap<Addr, CableManager<S>>,
    connections: HashSet<Connection>,
    close_channel_sender: CloseChannelSender,
    storage_fn: StorageFn<S>,
    pub ui: Arc<Mutex<Ui>>,
    exit: bool,
}

impl<S> App<S>
where
    S: Store,
{
    pub fn new(
        size: TermSize,
        storage_fn: StorageFn<S>,
        close_channel_sender: CloseChannelSender,
    ) -> Self {
        Self {
            abort_handles: Arc::new(Mutex::new(HashMap::new())),
            cables: HashMap::new(),
            connections: HashSet::new(),
            close_channel_sender,
            storage_fn,
            ui: Arc::new(Mutex::new(Ui::new(size))),
            exit: false,
        }
    }

    /// Listen for "close channel" messages and abort the associated task
    /// responsible for updating the UI with posts from the given channel.
    /// This prevents double-posting to the UI if a channel is left and then
    /// later rejoined.
    ///
    /// A "close channel" message is sent when the `close_channel()` handler
    /// is invoked.
    async fn launch_abort_listener(&mut self, mut close_channel_receiver: CloseChannelReceiver) {
        let abort_handles = self.abort_handles.clone();

        task::spawn(async move {
            while let Some(close_channel) = close_channel_receiver.next().await {
                let abort_handles = abort_handles.lock().await;
                if let Some(handle) = abort_handles.get(&close_channel) {
                    debug!("Aborting post display task for channel {:?}", close_channel);
                    handle.abort();
                }
            }
        });
    }

    /// Add the given cabal address (key) to the cable manager.
    pub fn add_cable(&mut self, addr: &Addr) {
        let s_addr = hex::to(addr);
        self.cables.insert(
            addr.to_vec(),
            CableManager::new(*(self.storage_fn)(&s_addr)),
        );
    }

    /// Return the address and manager for the active cable.
    pub async fn get_active_cable(&mut self) -> Option<(Addr, CableManager<S>)> {
        self.ui
            .lock()
            .await
            .get_active_address()
            .and_then(|addr| self.cables.get(addr).map(|c| (addr.clone(), c.clone())))
    }

    /// Set the address (key) of the active cabal.
    pub async fn set_active_address(&self, addr: &Addr) {
        self.ui.lock().await.set_active_address(addr);
    }

    /// Get the address (key) of the active cabal.
    pub async fn get_active_address(&self) -> Option<Addr> {
        self.ui.lock().await.get_active_address().cloned()
    }

    /// Handle the `/cabal` commands.
    ///
    /// Adds a new cabal, sets the active cabal or lists all known cabals.
    // TODO: Split this into multiple handler, one per subcommand.
    async fn cabal_handler(&mut self, args: Vec<String>) {
        match (args.get(1).map(|x| x.as_str()), args.get(2)) {
            (Some("add"), Some(hex_addr)) => {
                if let Some(addr) = hex::from(hex_addr) {
                    self.add_cable(&addr);
                    self.write_status(&format!("added cabal: {}", hex_addr))
                        .await;
                    self.set_active_address(&addr).await;
                    self.write_status(&format!("set active cabal to {}", hex_addr))
                        .await;
                } else {
                    self.write_status(&format!("invalid cabal address: {}", hex_addr))
                        .await;
                }
            }
            (Some("add"), None) => {
                self.write_status("usage: /cabal add ADDR").await;
            }
            (Some("set"), Some(s_addr)) => {
                if let Some(addr) = hex::from(s_addr) {
                    self.set_active_address(&addr).await;
                    self.write_status(&format!("set active cabal to {}", s_addr))
                        .await;
                } else {
                    self.write_status(&format!("invalid cabal address: {}", s_addr))
                        .await;
                }
            }
            (Some("set"), None) => {
                self.write_status("usage: /cabal set ADDR").await;
            }
            (Some("list"), _) => {
                for addr in self.cables.keys() {
                    let is_active = self
                        .get_active_address()
                        .await
                        .map(|x| &x == addr)
                        .unwrap_or(false);
                    let star = if is_active { "*" } else { "" };
                    self.write_status(&format!("{}{}", hex::to(addr), star))
                        .await;
                }
                if self.cables.is_empty() {
                    self.write_status("{ no cabals in list }").await;
                }
            }
            _ => {}
        }
    }

    /// Handle the `/channels` command.
    ///
    /// Prints a list of known channels for the active cable instance.
    async fn channels_handler(&mut self) {
        if let Some((_address, cable)) = self.get_active_cable().await {
            let mut ui = self.ui.lock().await;
            if let Some(channels) = cable.store.get_channels().await {
                for channel in channels {
                    ui.write_status(&format!("- {}", channel));
                }
            } else {
                ui.write_status("{ no known channels for the active cabal }");
            }
            ui.update();
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot list channels with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }
    }

    /// Handle the `/connect` command.
    ///
    /// Attempts a TCP connection to the given host:port.
    async fn connect_handler(&mut self, args: Vec<String>) {
        if self.get_active_address().await.is_none() {
            self.write_status(r#"no active cabal to bind this connection. use "/cabal add" first"#)
                .await;
        } else if let Some(tcp_addr) = args.get(1).cloned() {
            // Retrieve the active cable manager.
            let (_, cable) = self.get_active_cable().await.unwrap();

            let ui = self.ui.clone();

            // Register the connection.
            self.connections
                .insert(Connection::Connected(tcp_addr.clone()));

            // Attempt a TCP connection to the peer and invoke the
            // cable listener.
            task::spawn(async move {
                let stream = net::TcpStream::connect(tcp_addr.clone()).await?;

                // This block expression is needed to drop the lock and prevent
                // blocking of the UI.
                {
                    // Update the UI.
                    let mut ui = ui.lock().await;
                    ui.write_status(&format!("connected to {}", tcp_addr));
                    ui.update();
                }

                cable.listen(stream).await?;

                // Type inference fails without binding concretely to `Result`.
                Result::<(), Error>::Ok(())
            });
        } else {
            // Print usage example for the connect command.
            let mut ui = self.ui.lock().await;
            ui.write_status("usage: /connect HOST:PORT");
            ui.update();
        }
    }

    /// Handle the `/connections` command.
    ///
    /// Prints a list of active TCP connections.
    async fn connections_handler(&mut self) {
        let mut ui = self.ui.lock().await;
        for connection in self.connections.iter() {
            ui.write_status(&match connection {
                Connection::Connected(addr) => format!("connected to {}", addr),
                Connection::Listening(addr) => format!("listening on {}", addr),
            });
        }
        if self.connections.is_empty() {
            ui.write_status("{ no connections in list }");
        }
        ui.update();
    }

    /// Handle the `/delete` command.
    ///
    /// Deletes the most recently set nickname for the local peer.
    async fn delete_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((_address, mut cable)) = self.get_active_cable().await {
            if let Some("nick") = args.get(1).map(|arg| arg.as_str()) {
                if let Some((public_key, _private_key)) = cable.store.get_keypair().await {
                    if let Some((_name, hash)) =
                        cable.store.get_peer_name_and_hash(&public_key).await
                    {
                        cable.post_delete(vec![hash]).await?;
                        let mut ui = self.ui.lock().await;
                        ui.write_status("deleted most recent nickname");
                        ui.update();
                    } else {
                        let mut ui = self.ui.lock().await;
                        ui.write_status("no nickname found for the local peer");
                        ui.update();
                    }
                }
            } else {
                self.write_status("usage: /delete nick").await;
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot delete nickname with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }
        Ok(())
    }

    /// Handle the `/help` command.
    ///
    /// Prints a description and usage example for all commands.
    async fn help_handler(&mut self) {
        let mut ui = self.ui.lock().await;
        ui.write_status("/cabal add ADDR");
        ui.write_status("  add a cabal");
        ui.write_status("/cabal set ADDR");
        ui.write_status("  set the active cabal");
        ui.write_status("/cabal list");
        ui.write_status("  list all known cabals");
        ui.write_status("/channels");
        ui.write_status("  list all known channels");
        ui.write_status("/connections");
        ui.write_status("  list all known network connections");
        ui.write_status("/connect HOST:PORT");
        ui.write_status("  connect to a peer over tcp");
        ui.write_status("/delete nick");
        ui.write_status("  delete the most recent nick");
        ui.write_status("/join CHANNEL");
        ui.write_status("  join a channel (shorthand: /j CHANNEL)");
        ui.write_status("/listen PORT");
        ui.write_status("  listen for incoming tcp connections on 0.0.0.0");
        ui.write_status("/listen HOST:PORT");
        ui.write_status("  listen for incoming tcp connections");
        ui.write_status("/members CHANNEL");
        ui.write_status("  list all known members of the channel");
        ui.write_status("/topic");
        ui.write_status("  list the topic of the active channel");
        ui.write_status("/topic TOPIC");
        ui.write_status("  set the topic of the active channel");
        ui.write_status("/whoami");
        ui.write_status("  list the local public key as a hex string");
        ui.write_status("/win INDEX");
        ui.write_status("  change the active window (shorthand: /w INDEX)");
        ui.write_status("/exit");
        ui.write_status("  exit the cabal process");
        ui.write_status("/quit");
        ui.write_status("  exit the cabal process (shorthand: /q)");
        ui.update();
    }

    /// Handle the `/join` and `/j` commands.
    ///
    /// Sets the active window of the UI, publishes a `post/join` if the local
    /// peer is not already a channel member, creates a channel time range
    /// request and updates the UI with stored and received posts.
    async fn join_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((address, mut cable)) = self.get_active_cable().await {
            if let Some(channel) = args.get(1) {
                // Check if the local peer is already a member of this channel.
                // If not, publish a `post/join` post.
                if let Some((public_key, _private_key)) = cable.store.get_keypair().await {
                    if !cable.store.is_channel_member(channel, &public_key).await {
                        // TODO: Match on validation error and display to user.
                        cable.post_join(channel).await?;
                    }
                }

                let mut ui = self.ui.lock().await;
                let channel_window_index = ui.get_window_index(&address, channel);

                // Define the window index.
                //
                // First check if a window has previously been created for the
                // given address / channel combination. If so, return the
                // index. Otherwise, add a new window and return the index.
                let index = channel_window_index
                    .unwrap_or_else(|| ui.add_window(address.clone(), channel.clone()));

                let ch = channel.clone();

                ui.set_active_index(index);
                ui.update();
                // The UI remains locked if not explicitly dropped here.
                drop(ui);

                // Define the channel options.
                let opts = ChannelOptions {
                    channel: ch.clone(),
                    time_start: time::two_weeks_ago()?,
                    time_end: 0,
                    limit: 4096,
                };

                let store = cable.store.clone();
                let ui = self.ui.clone();
                let mut ui = ui.lock().await;

                // Open the channel and update the UI with stored and received
                // text posts; only if this action has not been performed
                // previously.
                //
                // The window index is used as a proxy for "channel has been
                // initialised".
                if channel_window_index.is_none() {
                    ui.write_status(&format!("joined channel {}", channel));
                    ui.update();

                    let mut stored_posts_stream = cable.store.get_posts(&opts).await;
                    while let Some(post_stream) = stored_posts_stream.next().await {
                        if let Ok(post) = post_stream {
                            let timestamp = post.header.timestamp;
                            let public_key = post.header.public_key;
                            let nickname = store
                                .get_peer_name_and_hash(&public_key)
                                .await
                                .map(|(nick, _hash)| nick);

                            if let PostBody::Text { channel, text } = post.body {
                                if let Some(window) = ui.get_window(&address, &channel) {
                                    window.insert(timestamp, Some(public_key), nickname, &text);
                                    ui.update();
                                }
                            } else if let PostBody::Topic { channel, topic } = post.body {
                                if let Some(window) = ui.get_window(&address, &channel) {
                                    window.update_topic(topic);
                                    ui.update();
                                }
                            }
                        }
                    }
                    drop(stored_posts_stream);

                    // Create an abort handle and add it to the local map.
                    //
                    // This allows the `display_posts` task to be aborted
                    // when the channel is left, thereby preventing double
                    // posting to the UI if the channel is later rejoined.
                    let (abort_handle, abort_registration) = AbortHandle::new_pair();
                    self.abort_handles
                        .lock()
                        .await
                        .insert(channel.to_owned(), abort_handle);

                    let store = cable.store.clone();

                    let ui = self.ui.clone();
                    let display_posts = async move {
                        let mut stream = cable
                            .open_channel(&opts)
                            .await
                            // TODO: Can we handle this unwrap another way?
                            .unwrap();

                        while let Some(post_stream) = stream.next().await {
                            if let Ok(post) = post_stream {
                                let timestamp = post.header.timestamp;
                                let public_key = post.header.public_key;
                                let nickname = store
                                    .get_peer_name_and_hash(&public_key)
                                    .await
                                    .map(|(nick, _hash)| nick);

                                if let PostBody::Text { channel, text } = post.body {
                                    let mut ui = ui.lock().await;
                                    if let Some(window) = ui.get_window(&address, &channel) {
                                        window.insert(timestamp, Some(public_key), nickname, &text);
                                        ui.update();
                                    }
                                } else if let PostBody::Topic { channel, topic } = post.body {
                                    let mut ui = ui.lock().await;
                                    if let Some(window) = ui.get_window(&address, &channel) {
                                        window.update_topic(topic);
                                        ui.update();
                                    }
                                }
                            }
                        }
                    };

                    task::spawn(Abortable::new(display_posts, abort_registration));
                }
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /join CHANNEL");
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot join channel with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }

        Ok(())
    }

    /// Handle the `/leave` command.
    ///
    /// Cancels any active outbound channel time range requests for the
    /// given channel and publishes a `post/leave`.
    async fn leave_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((address, mut cable)) = self.get_active_cable().await {
            if let Some(channel) = args.get(1) {
                if let Some(channels) = cable.store.get_channels().await {
                    // Avoid closing and leaving a channel that isn't known to the
                    // local peer.
                    if channels.contains(channel) {
                        // Cancel any active outbound channel time range requests
                        // for this channel.
                        cable.close_channel(channel).await?;

                        // Check if the local peer is a member of this channel.
                        // If so, publish a `post/leave` post.
                        if let Some((public_key, _private_key)) = cable.store.get_keypair().await {
                            if cable.store.is_channel_member(channel, &public_key).await {
                                // TODO: Match on validation error and display to user.
                                cable.post_leave(channel).await?;
                            }
                        }

                        self.close_channel_sender.send(channel.to_owned()).await?;

                        let mut ui = self.ui.lock().await;
                        // Remove the window associated with the given channel.
                        if let Some(index) = ui.get_window_index(&address, channel) {
                            ui.remove_window(index)
                        }
                        // Return to the home / status window.
                        ui.set_active_index(0);
                        ui.write_status(&format!("left channel {}", channel));
                        ui.update();
                    }
                } else {
                    let mut ui = self.ui.lock().await;
                    ui.write_status(&format!(
                        "not currently a member of channel {}; no action taken",
                        channel
                    ));
                    ui.update();
                }
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /leave CHANNEL");
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot leave channel with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }

        Ok(())
    }

    /// Handle the `/listen` command.
    ///
    /// Deploys a TCP server on the given host:port, listens for incoming
    /// connections and passes any resulting streams to the cable manager.
    async fn listen_handler(&mut self, args: Vec<String>) {
        // Retrieve the active cable address (aka. key).
        if self.get_active_address().await.is_none() {
            self.write_status(r#"no active cabal to bind this connection. use "/cabal add" first"#)
                .await;
        } else if let Some(mut tcp_addr) = args.get(1).cloned() {
            // Format the TCP address if a host was not supplied.
            if !tcp_addr.contains(':') {
                tcp_addr = format!("0.0.0.0:{}", tcp_addr);
            }

            // Retrieve the active cable manager.
            let (_, cable) = self.get_active_cable().await.unwrap();

            // Register the listener.
            self.connections
                .insert(Connection::Listening(tcp_addr.clone()));

            let ui = self.ui.clone();

            task::spawn(async move {
                let listener = net::TcpListener::bind(tcp_addr.clone()).await.unwrap();

                // Update the UI.
                let mut ui = ui.lock().await;
                ui.write_status(&format!("listening on {}", tcp_addr));
                ui.update();
                drop(ui);

                debug!("Listening for incoming TCP connections...");

                // Listen for incoming TCP connections and spawn a
                // cable listener for each stream.
                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    debug!("Received an incoming TCP connection");
                    if let Ok(stream) = stream {
                        let cable = cable.clone();
                        task::spawn(async move {
                            if let Err(err) = cable.listen(stream).await {
                                error!("Cable stream listener error: {}", err);
                            }
                        });
                    }
                }
            });
        } else {
            // Print usage example for the listen command.
            let mut ui = self.ui.lock().await;
            ui.write_status("usage: /listen (ADDR:)PORT");
            ui.update();
        }
    }

    /// Handle the `/members` command.
    ///
    /// Prints a list of known members of a channel. If this handler is invoked
    /// from an active channel window, the members of that channel will be
    /// printed. Otherwise, the handler can be invoked with a specific channel
    /// name as an argument; this is useful for printing channel members when
    /// the status window is active.
    async fn members_handler(&mut self, args: Vec<String>) {
        if let Some((_address, cable)) = self.get_active_cable().await {
            if let Some(channel) = args.get(1) {
                let mut ui = self.ui.lock().await;

                if let Some(members) = cable.store.get_channel_members(channel).await {
                    for member in members {
                        // Retrieve and print the nick for each member's
                        // public key.
                        if let Some((name, _hash)) =
                            cable.store.get_peer_name_and_hash(&member).await
                        {
                            ui.write_status(&format!("  {}", name));
                        } else {
                            // Fall back to the public key (formatted as a
                            // hex string) if no nick is known.
                            ui.write_status(&format!("  {}", hex::to(&member)));
                        }
                    }
                } else {
                    ui.write_status(
                        "{ no known channel members for the active cabal and channel }",
                    );
                }
                ui.update();
            } else {
                // No args were passed to the `/members` handler. Attempt to
                // determine the channel for the active window and print the
                // members.
                let mut ui = self.ui.lock().await;
                let index = ui.get_active_index();
                // Don't attempt to retrieve and print channel members if the
                // status window is active.
                if index != 0 {
                    let window = ui.get_active_window();
                    if let Some(members) = cable.store.get_channel_members(&window.channel).await {
                        for member in members {
                            // Retrieve and print the nick for each member's
                            // public key.
                            if let Some((name, _hash)) =
                                cable.store.get_peer_name_and_hash(&member).await
                            {
                                ui.write_status(&format!("  {}", name));
                            } else {
                                // Fall back to the public key (formatted as a
                                // hex string) if no nick is known.
                                ui.write_status(&format!("  {}", hex::to(&member)));
                            }
                        }
                    } else {
                        ui.write_status(
                            "{ no known channel members for the active cabal and channel }",
                        );
                    }
                    ui.update();
                }
            };
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot list channel members with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }
    }

    /// Handle the `/nick` command.
    ///
    /// Set the nickname for the local peer.
    async fn nick_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((_address, mut cable)) = self.get_active_cable().await {
            if let Some(nick) = args.get(1) {
                let mut ui = self.ui.lock().await;
                let _hash = cable.post_info_name(nick).await?;
                ui.write_status(&format!("nickname set to {:?}", nick));
                ui.update();
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /nick NAME");
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot assign nickname with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }

        Ok(())
    }

    /// Handle the `/topic` command.
    ///
    /// Sets the topic of the active channel.
    async fn topic_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((_address, mut cable)) = self.get_active_cable().await {
            if args.get(1).is_some() {
                // Get all arguments that come after the `/topic` argument.
                let topic: String = args[1..].join(" ");
                let mut ui = self.ui.lock().await;
                let active_channel = ui.get_active_window().channel.to_owned();
                if active_channel != "!status" {
                    cable.post_topic(&active_channel, &topic).await?;
                    ui.write_status(&format!(
                        "topic set to {:?} for channel {:?}",
                        topic, active_channel
                    ));
                    ui.update();
                } else {
                    ui.write_status("topic cannot be set for !status window");
                    ui.update();
                }
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /topic TOPIC");
                ui.update();
            }
        }

        Ok(())
    }

    /// Handle the `/whoami` command.
    ///
    /// Prints the hex-encoded public key of the local peer.
    async fn whoami_handler(&mut self) {
        if let Some((_address, cable)) = self.get_active_cable().await {
            if let Some((public_key, _private_key)) = cable.store.get_keypair().await {
                let mut ui = self.ui.lock().await;
                ui.write_status(&format!("  {}", hex::to(&public_key)));
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format!(
                "{}{}",
                "cannot list the local public key with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ));
            ui.update();
        }
    }

    /// Handle the `/win` and `/w` commands.
    ///
    /// Sets the active window of the UI.
    async fn win_handler(&mut self, args: Vec<String>) {
        let mut ui = self.ui.lock().await;
        if let Some(index) = args.get(1) {
            if let Ok(i) = index.parse() {
                ui.set_active_index(i);
                ui.update();
            } else {
                ui.write_status("window index must be a number");
                ui.update();
            }
        } else {
            ui.write_status("usage: /win INDEX");
            ui.update();
        }
    }

    /// Parse UI input and invoke the appropriate handler.
    pub async fn handle(&mut self, line: &str) -> Result<(), Error> {
        let args = line
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        if args.is_empty() {
            return Ok(());
        }

        match args.get(0).unwrap().as_str() {
            "/cabal" => {
                self.write_status(line).await;
                self.cabal_handler(args).await;
            }
            "/channels" => {
                self.write_status(line).await;
                self.channels_handler().await;
            }
            "/connect" => {
                self.write_status(line).await;
                self.connect_handler(args).await;
            }
            "/connections" => {
                self.write_status(line).await;
                self.connections_handler().await;
            }
            "/delete" => {
                self.write_status(line).await;
                self.delete_handler(args).await?;
            }
            "/help" => {
                self.write_status(line).await;
                self.help_handler().await;
            }
            "/join" | "/j" => {
                self.join_handler(args).await?;
            }
            "/leave" => {
                self.leave_handler(args).await?;
            }
            "/listen" => {
                self.write_status(line).await;
                self.listen_handler(args).await;
            }
            "/members" => {
                self.write_status(line).await;
                self.members_handler(args).await;
            }
            "/nick" => {
                self.write_status(line).await;
                self.nick_handler(args).await?;
            }
            "/topic" => {
                self.write_status(line).await;
                self.topic_handler(args).await?;
            }
            "/quit" | "/exit" | "/q" => {
                self.write_status(line).await;
                self.exit = true;
            }
            "/whoami" => {
                self.write_status(line).await;
                self.whoami_handler().await;
            }
            "/win" | "/w" => {
                self.win_handler(args).await;
            }
            x => {
                if x.starts_with('/') {
                    self.write_status(line).await;
                    self.write_status(&format!("no such command: {}", x)).await;
                } else {
                    self.post(&line.trim_end().to_string()).await?;
                }
            }
        }

        Ok(())
    }

    /// Post the given text message to the channel and cabal associated with
    /// the active UI window.
    pub async fn post(&mut self, msg: &String) -> Result<(), Error> {
        let mut ui = self.ui.lock().await;
        let w = ui.get_active_window();
        if w.channel == "!status" {
            ui.write_status("can't post text in status channel. see /help for command list");
            ui.update();
        } else {
            let cable = self.cables.get_mut(&w.address).unwrap();
            // TODO: Match on validation error and display to user.
            cable.post_text(&w.channel, msg).await?;
        }
        Ok(())
    }

    /// Run the application.
    ///
    /// Handle input and update the UI.
    pub async fn run(
        &mut self,
        mut reader: Box<dyn Read>,
        close_channel_receiver: CloseChannelReceiver,
    ) -> Result<(), Error> {
        self.launch_abort_listener(close_channel_receiver).await;

        self.ui.lock().await.update();
        self.write_status_banner().await;

        let mut buf = vec![0];
        while !self.exit {
            // Parse input from stdin.
            reader.read_exact(&mut buf).unwrap();
            let lines = {
                let mut ui = self.ui.lock().await;
                ui.input.putc(buf[0]);
                ui.update();
                let mut lines = vec![];
                while let Some(event) = ui.input.next_event() {
                    match event {
                        // TODO: Handle PageUp and PageDown.
                        InputEvent::KeyCode(KeyCode::PageUp) => {}
                        InputEvent::KeyCode(KeyCode::PageDown) => {}
                        InputEvent::KeyCode(_) => {}
                        InputEvent::Line(line) => {
                            lines.push(line);
                        }
                    }
                }
                lines
            };

            // Invoke the handler for each line of input.
            for line in lines {
                self.handle(&line).await?;
                if self.exit {
                    break;
                }
            }
        }
        self.ui.lock().await.finish();

        Ok(())
    }

    /// Update the UI.
    pub async fn update(&self) {
        self.ui.lock().await.update();
    }

    /// Write the given message to the UI.
    pub async fn write_status(&self, msg: &str) {
        let mut ui = self.ui.lock().await;
        ui.write_status(msg);
        ui.update();
    }

    /// Write the welcome banner to the status window.
    pub async fn write_status_banner(&mut self) {
        // Include the welcome banner at compile time.
        let banner = include_str!("../welcome.txt");

        let mut ui = self.ui.lock().await;
        for line in banner.lines() {
            ui.write_status(line)
        }
        ui.update();
    }
}

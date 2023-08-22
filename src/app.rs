use std::collections::{HashMap, HashSet};
use std::io::Read;

use async_std::{
    net,
    prelude::*,
    sync::{Arc, Mutex},
    task,
};
use cable::{error::Error, post::PostBody, ChannelOptions};
use cable_core::{CableManager, Store};
use terminal_keycode::KeyCode;

use crate::{
    hex,
    input::InputEvent,
    ui::{Addr, TermSize, Ui},
};

type StorageFn<S> = Box<dyn Fn(&str) -> Box<S>>;

/// A TCP connection and associated address (host:post).
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum Connection {
    Connected(String),
    Listening(String),
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub struct App<S: Store> {
    cables: HashMap<Addr, CableManager<S>>,
    connections: HashSet<Connection>,
    storage_fn: StorageFn<S>,
    pub ui: Arc<Mutex<Ui>>,
    exit: bool,
}

impl<S> App<S>
where
    S: Store,
{
    pub fn new(size: TermSize, storage_fn: StorageFn<S>) -> Self {
        Self {
            cables: HashMap::new(),
            connections: HashSet::new(),
            storage_fn,
            ui: Arc::new(Mutex::new(Ui::new(size))),
            exit: false,
        }
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
                    self.write_status(&format!["added cabal: {}", hex_addr])
                        .await;
                    self.set_active_address(&addr).await;
                    self.write_status(&format!["set active cabal to {}", hex_addr])
                        .await;
                } else {
                    self.write_status(&format!["invalid cabal address: {}", hex_addr])
                        .await;
                }
            }
            (Some("add"), None) => {
                self.write_status("usage: /cabal add ADDR").await;
            }
            (Some("set"), Some(s_addr)) => {
                if let Some(addr) = hex::from(s_addr) {
                    self.set_active_address(&addr).await;
                    self.write_status(&format!["set active cabal to {}", s_addr])
                        .await;
                } else {
                    self.write_status(&format!["invalid cabal address: {}", s_addr])
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
                    self.write_status(&format!["{}{}", hex::to(addr), star])
                        .await;
                }
                if self.cables.is_empty() {
                    self.write_status("{ no cabals in list }").await;
                }
            }
            _ => {}
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
                    ui.write_status(&format!["connected to {}", tcp_addr]);
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
                Connection::Connected(addr) => format!["connected to {}", addr],
                Connection::Listening(addr) => format!["listening on {}", addr],
            });
        }
        if self.connections.is_empty() {
            ui.write_status("{ no connections in list }");
        }
        ui.update();
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
        ui.write_status("/connections");
        ui.write_status("  list all known network connections");
        ui.write_status("/connect HOST:PORT");
        ui.write_status("  connect to a peer over tcp");
        ui.write_status("/listen PORT");
        ui.write_status("  listen for incoming tcp connections on 0.0.0.0");
        ui.write_status("/listen HOST:PORT");
        ui.write_status("  listen for incoming tcp connections");
        ui.write_status("/join CHANNEL");
        ui.write_status("  join a channel (shorthand: /j CHANNEL)");
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
                if let Some(keypair) = cable.store.get_keypair().await? {
                    let public_key = keypair.0;
                    if !cable.store.is_channel_member(channel, &public_key).await? {
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

                /*
                // Query the size of the UI in order to define the channel
                // options limit.
                let limit = {
                    //let mut ui = self.ui.lock().await;
                    ui.set_active_index(index);
                    ui.update();
                    ui.get_size().1 as u64
                };
                */
                ui.set_active_index(index);
                ui.update();
                let ui = self.ui.clone();

                // Define the channel options.
                let opts = ChannelOptions {
                    channel: ch.clone(),
                    time_start: 0,
                    time_end: 0,
                    //limit,
                    limit: 4096,
                };

                let mut stored_posts_stream = cable.store.get_posts(&opts).await.unwrap();
                while let Some(post_stream) = stored_posts_stream.next().await {
                    if let Ok(post) = post_stream {
                        let timestamp = post.header.timestamp;

                        if let PostBody::Text { text, channel } = post.body {
                            let mut ui = ui.lock().await;
                            if let Some(window) = ui.get_window(&address, &channel) {
                                window.insert(timestamp, &text);
                                ui.update();
                            }
                        }
                    }
                }
                drop(stored_posts_stream);

                // Open the channel and update the UI with received text posts;
                // only if this action has not been performed previously.
                //
                // The window index is used as a proxy for "channel has been
                // initialised".
                if channel_window_index.is_none() {
                    task::spawn(async move {
                        let mut stream = cable
                            .open_channel(&opts)
                            .await
                            // TODO: Can we handle this unwrap another way?
                            .unwrap();

                        while let Some(post_stream) = stream.next().await {
                            if let Ok(post) = post_stream {
                                let timestamp = post.header.timestamp;

                                if let PostBody::Text { text, channel } = post.body {
                                    let mut ui = ui.lock().await;
                                    if let Some(window) = ui.get_window(&address, &channel) {
                                        window.insert(timestamp, &text);
                                        ui.update();
                                    }
                                }
                            }
                        }
                    });
                }
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /join CHANNEL");
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format![
                "{}{}",
                "cannot join channel with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ]);
            ui.update();
        }

        Ok(())
    }

    /// Handle the `/leave` command.
    ///
    /// Cancels any active outbound channel time range requests for the
    /// given channel and publishes a `post/leave`.
    async fn leave_handler(&mut self, args: Vec<String>) -> Result<(), Error> {
        if let Some((_address, mut cable)) = self.get_active_cable().await {
            if let Some(channel) = args.get(1) {
                // Cancel any active outbound channel time range requests
                // for this channel.
                cable.close_channel(channel).await?;

                // Check if the local peer is a member of this channel.
                // If so, publish a `post/leave` post.
                if let Some(keypair) = cable.store.get_keypair().await? {
                    let public_key = keypair.0;
                    if cable.store.is_channel_member(channel, &public_key).await? {
                        cable.post_leave(channel).await?;
                    }
                }
            } else {
                let mut ui = self.ui.lock().await;
                ui.write_status("usage: /leave CHANNEL");
                ui.update();
            }
        } else {
            let mut ui = self.ui.lock().await;
            ui.write_status(&format![
                "{}{}",
                "cannot leave channel with no active cabal set.",
                " add a cabal with \"/cabal add\" first",
            ]);
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
                tcp_addr = format!["0.0.0.0:{}", tcp_addr];
            }

            // Retrieve the active cable manager.
            let (_, cable) = self.get_active_cable().await.unwrap();

            // Register the listener.
            self.connections
                .insert(Connection::Listening(tcp_addr.clone()));

            let ui = self.ui.clone();

            task::spawn(async move {
                let listener = net::TcpListener::bind(tcp_addr.clone()).await?;

                // This block expression is needed to drop the lock and prevent
                // blocking of the UI.
                {
                    // Update the UI.
                    let mut ui = ui.lock().await;
                    ui.write_status(&format!["listening on {}", tcp_addr]);
                    ui.update();
                }

                // Listen for incoming TCP connections and spawn a
                // cable listener for each stream.
                let mut incoming = listener.incoming();
                while let Some(stream) = incoming.next().await {
                    if let Ok(stream) = stream {
                        let cable = cable.clone();
                        task::spawn(async move {
                            cable.listen(stream).await.unwrap();
                        });
                    }
                }

                // Type inference fails without binding concretely to `Result`.
                Result::<(), Error>::Ok(())
            });
        } else {
            // Print usage example for the listen command.
            let mut ui = self.ui.lock().await;
            ui.write_status("usage: /listen (ADDR:)PORT");
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
            "/help" => {
                self.write_status(line).await;
                self.help_handler().await;
            }
            "/quit" | "/exit" | "/q" => {
                self.write_status(line).await;
                self.exit = true;
            }
            "/win" | "/w" => {
                self.win_handler(args).await;
            }
            "/join" | "/j" => {
                self.join_handler(args).await?;
            }
            "/cabal" => {
                self.write_status(line).await;
                self.cabal_handler(args).await;
            }
            "/connections" => {
                self.write_status(line).await;
                self.connections_handler().await;
            }
            "/connect" => {
                self.write_status(line).await;
                self.connect_handler(args).await;
            }
            "/leave" => {
                self.leave_handler(args).await?;
            }
            "/listen" => {
                self.write_status(line).await;
                self.listen_handler(args).await;
            }
            x => {
                if x.starts_with('/') {
                    self.write_status(line).await;
                    self.write_status(&format!["no such command: {}", x]).await;
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
            cable.post_text(&w.channel, msg).await?;
        }
        Ok(())
    }

    /// Run the application.
    ///
    /// Handle input and update the UI.
    pub async fn run(&mut self, mut reader: Box<dyn Read>) -> Result<(), Error> {
        self.ui.lock().await.update();

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
}

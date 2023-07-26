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
    ui::{Addr, TermSize, UI},
};

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum Connection {
    Connected(String),
    Listening(String),
}

// TODO: Make this wayyy less hacky.
async fn open_channel_and_display_text_posts<S: Store>(
    channel: String,
    limit: u64,
    address: Vec<u8>,
    mut cable: CableManager<S>,
    m_ui: Arc<async_std::sync::Mutex<UI>>,
) {
    task::spawn(async move {
        let mut stream = cable
            .open_channel(&ChannelOptions {
                channel: channel.clone(),
                time_start: 0,
                time_end: 0,
                limit,
            })
            .await
            .unwrap();
        while let Some(r) = stream.next().await {
            match r.unwrap().body {
                PostBody::Text {
                    //timestamp,
                    text: _,
                    channel: _,
                } => {
                    let mut ui = m_ui.lock().await;
                    if let Some(_w) = ui.get_window(&address, &channel) {
                        //w.insert(timestamp, &String::from_utf8_lossy(&text));
                        // TODO: Get timestamp...
                        //w.insert(timestamp, &text);
                        ui.update();
                    }
                }
                _ => {}
            }
        }
    });
}

pub struct App<S: Store> {
    cables: HashMap<Addr, CableManager<S>>,
    connections: HashSet<Connection>,
    storage_fn: Box<dyn Fn(&str) -> Box<S>>,
    pub ui: Arc<Mutex<UI>>,
    exit: bool,
}

impl<S> App<S>
where
    S: Store,
{
    pub fn new(size: TermSize, storage_fn: Box<dyn Fn(&str) -> Box<S>>) -> Self {
        Self {
            cables: HashMap::new(),
            connections: HashSet::new(),
            storage_fn,
            ui: Arc::new(Mutex::new(UI::new(size))),
            exit: false,
        }
    }

    pub async fn run(&mut self, mut reader: Box<dyn Read>) -> Result<(), Error> {
        self.ui.lock().await.update();
        let mut buf = vec![0];
        while !self.exit {
            reader.read_exact(&mut buf).unwrap();
            let lines = {
                let mut ui = self.ui.lock().await;
                ui.input.putc(buf[0]);
                ui.update();
                let mut lines = vec![];
                while let Some(event) = ui.input.next() {
                    match event {
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
                let mut ui = self.ui.lock().await;
                ui.write_status("available commands: TODO");
                ui.update();
            }
            "/quit" | "/exit" | "/q" => {
                self.write_status(line).await;
                self.exit = true;
            }
            "/win" | "/w" => {
                let i: usize = args.get(1).unwrap().parse().unwrap();
                let mut ui = self.ui.lock().await;
                ui.set_active_index(i);
                ui.update();
            }
            "/join" | "/j" => {
                if let Some((address, cable)) = self.get_active_cable().await {
                    if let Some(channel) = args.get(1) {
                        let ch = channel.clone();
                        let limit = {
                            let mut ui = self.ui.lock().await;
                            let i = ui.add_window(address.clone(), ch.clone());
                            ui.set_active_index(i);
                            ui.update();
                            ui.get_size().1 as u64
                        };
                        let m_ui = self.ui.clone();
                        let addr = address.clone();
                        open_channel_and_display_text_posts(ch, limit, addr, cable.clone(), m_ui)
                            .await;
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
            }
            "/cabal" => {
                self.write_status(line).await;
                match (args.get(1).map(|x| x.as_str()), args.get(2)) {
                    (Some("add"), Some(s_addr)) => {
                        if let Some(addr) = hex::from(s_addr) {
                            self.add_cable(&addr);
                            self.write_status(&format!["added cabal: {}", s_addr]).await;
                            self.set_active_address(&addr).await;
                            self.write_status(&format!["set active cabal to {}", s_addr])
                                .await;
                        } else {
                            self.write_status(&format!["invalid cabal address: {}", s_addr])
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
            "/connections" => {
                self.write_status(line).await;
                let mut ui = self.ui.lock().await;
                for c in self.connections.iter() {
                    ui.write_status(&match c {
                        Connection::Connected(addr) => format!["connected to {}", addr],
                        Connection::Listening(addr) => format!["listening on {}", addr],
                    });
                }
                if self.connections.is_empty() {
                    ui.write_status("{ no connections in list }");
                }
                ui.update();
            }
            "/connect" => {
                self.write_status(line).await;
                if self.get_active_address().await.is_none() {
                    self.write_status(
                        r#"no active cabal to bind this connection. use "/cabal add" first"#,
                    )
                    .await;
                } else if let Some(tcp_addr) = args.get(1).cloned() {
                    let (_, cable) = self.get_active_cable().await.unwrap();
                    self.connections
                        .insert(Connection::Connected(tcp_addr.clone()));
                    let mui = self.ui.clone();
                    task::spawn(async move {
                        let stream = net::TcpStream::connect(tcp_addr.clone()).await?;
                        {
                            let mut ui = mui.lock().await;
                            ui.write_status(&format!["connected to {}", tcp_addr]);
                            ui.update();
                        }
                        cable.listen(stream).await?;
                        let r: Result<(), Error> = Ok(());
                        r
                    });
                } else {
                    let mut ui = self.ui.lock().await;
                    ui.write_status("usage: /connect HOST:PORT");
                    ui.update();
                }
            }
            "/listen" => {
                self.write_status(line).await;
                if self.get_active_address().await.is_none() {
                    self.write_status(
                        r#"no active cabal to bind this connection. use "/cabal add" first"#,
                    )
                    .await;
                } else if let Some(mut tcp_addr) = args.get(1).cloned() {
                    if !tcp_addr.contains(":") {
                        tcp_addr = format!["0.0.0.0:{}", tcp_addr];
                    }
                    let (_, cable) = self.get_active_cable().await.unwrap();
                    self.connections
                        .insert(Connection::Listening(tcp_addr.clone()));
                    let mui = self.ui.clone();
                    task::spawn(async move {
                        let listener = net::TcpListener::bind(tcp_addr.clone()).await?;
                        {
                            let mut ui = mui.lock().await;
                            ui.write_status(&format!["listening on {}", tcp_addr]);
                            ui.update();
                        }
                        let mut incoming = listener.incoming();
                        while let Some(rstream) = incoming.next().await {
                            let stream = rstream.unwrap();
                            let client = cable.clone();
                            task::spawn(async move {
                                client.listen(stream).await.unwrap();
                            });
                        }
                        let r: Result<(), Error> = Ok(());
                        r
                    });
                } else {
                    let mut ui = self.ui.lock().await;
                    ui.write_status("usage: /listen (ADDR:)PORT");
                    ui.update();
                }
            }
            x => {
                if x.starts_with("/") {
                    self.write_status(line).await;
                    self.write_status(&format!["no such command: {}", x]).await;
                } else {
                    self.post(&line.trim_end().to_string()).await?;
                }
            }
        }
        Ok(())
    }

    pub fn add_cable(&mut self, addr: &Addr) {
        let s_addr = hex::to(addr);
        self.cables.insert(
            addr.to_vec(),
            CableManager::new(*(self.storage_fn)(&s_addr)),
        );
    }

    pub async fn set_active_address(&self, addr: &Addr) {
        self.ui.lock().await.set_active_address(addr);
    }

    pub async fn get_active_address(&self) -> Option<Addr> {
        self.ui.lock().await.get_active_address().cloned()
    }

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

    pub async fn get_active_cable(&mut self) -> Option<(Addr, CableManager<S>)> {
        self.ui
            .lock()
            .await
            .get_active_address()
            .and_then(|addr| self.cables.get(addr).map(|c| (addr.clone(), c.clone())))
    }

    pub async fn write_status(&self, msg: &str) {
        let mut ui = self.ui.lock().await;
        ui.write_status(msg);
        ui.update();
    }

    pub async fn update(&self) {
        self.ui.lock().await.update();
    }
}

use async_std::{prelude::*,task,net,sync::{Arc,Mutex}};
use std::collections::{HashMap,HashSet};
use cable::{Cable,Error,Store,ChannelOptions,PostBody};
use crate::{ui::{UI,Addr,TermSize},hex};
use std::io::Read;

#[derive(Debug,Clone,Hash,Eq,PartialEq)]
enum Connection {
  Connected(String),
  Listening(String),
}

pub struct App<S: Store> {
  cables: HashMap<Addr,Cable<S>>,
  connections: HashSet<Connection>,
  storage_fn: Box<dyn Fn (&str) -> Box<S>>,
  active_addr: Option<Addr>,
  pub ui: Arc<Mutex<UI>>,
  exit: bool,
}

impl<S> App<S> where S: Store {
  pub fn new(size: TermSize, storage_fn: Box<dyn Fn (&str) -> Box<S>>) -> Self {
    Self {
      cables: HashMap::new(),
      connections: HashSet::new(),
      storage_fn,
      active_addr: None,
      ui: Arc::new(Mutex::new(UI::new(size))),
      exit: false,
    }
  }
  pub async fn run(&mut self, mut reader: Box<dyn Read>) -> Result<(),Error> {
    self.ui.lock().await.update();
    let mut buf = vec![0];
    while !self.exit {
      reader.read_exact(&mut buf).unwrap();
      let lines = {
        let mut ui = self.ui.lock().await;
        ui.input.putc(buf[0]);
        ui.update();
        let mut lines = vec![];
        while let Some(line) = ui.input.get_next_line() {
          lines.push(line);
        }
        lines
      };
      for line in lines {
        self.handle(&line).await?;
        if self.exit { break }
      }
    }
    self.ui.lock().await.finish();
    Ok(())
  }
  pub async fn handle(&mut self, line: &str) -> Result<(),Error> {
    let args = line.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
    if args.is_empty() { return Ok(()) }
    match args.get(0).unwrap().as_str() {
      "/help" => {
        self.write_status(line).await;
        let mut ui = self.ui.lock().await;
        ui.write_status("available commands: TODO");
        ui.update();
      },
      "/quit" | "/exit" | "/q" => {
        self.write_status(line).await;
        self.exit = true;
      },
      "/win" | "/w" => {
        let i: usize = args.get(1).unwrap().parse().unwrap();
        let mut ui = self.ui.lock().await;
        ui.set_active_index(i);
        ui.update();
      },
      "/join" | "/j" => {
        if let Some(address) = self.active_addr.clone() {
          if let Some(channel) = args.get(1) {
            let ch = channel.as_bytes().to_vec();
            let limit = {
              let mut ui = self.ui.lock().await;
              let i = ui.add_window(address.clone(), ch.clone());
              ui.set_active_index(i);
              ui.update();
              ui.get_size().1 as usize
            };
            let mut cable = self.get_active_cable().await.unwrap();
            let m_ui = self.ui.clone();
            task::spawn(async move {
              let mut stream = cable.open_channel(&ChannelOptions {
                channel: ch.clone(),
                time_start: 0,
                time_end: 0,
                limit,
              }).await.unwrap();
              while let Some(r) = stream.next().await {
                match r.unwrap().body {
                  PostBody::Text { timestamp, text, channel: _ } => {
                    let mut ui = m_ui.lock().await;
                    if let Some(w) = ui.get_window(&address, &ch) {
                      if let Ok(s) = String::from_utf8(text.clone()) {
                        w.insert(timestamp, &s);
                      } else {
                        w.insert(timestamp, &format!["{:?}", text]);
                      }
                      ui.update();
                    }
                  },
                  _ => {},
                }
              }
            });
          } else {
            let mut ui = self.ui.lock().await;
            ui.write_status("usage: /join CHANNEL");
            ui.update();
          }
        } else {
          let mut ui = self.ui.lock().await;
          ui.write_status(&format!["{}{}",
            "cannot join channel with no active cabal set.",
            " add a cabal with \"/cabal add\" first",
          ]);
          ui.update();
        }
      },
      "/cabal" => {
        self.write_status(line).await;
        match (args.get(1).map(|x| x.as_str()), args.get(2)) {
          (Some("add"),Some(s_addr)) => {
            if let Some(addr) = hex::from(s_addr) {
              self.add_cable(&addr);
              self.write_status(&format!["added cabal: {}", s_addr]).await;
              if self.active_addr.is_none() {
                self.set_active_address(&addr);
                self.write_status(&format!["set active cabal to {}", s_addr]).await;
              }
            } else {
              self.write_status(&format!["invalid cabal address: {}", s_addr]).await;
            }
          },
          (Some("list"),_) => {
            let mut ui = self.ui.lock().await;
            for addr in self.cables.keys() {
              let star = if self.active_addr.as_ref().map(|x| x == addr).unwrap_or(false) { "*" } else { "" };
              ui.write_status(&format!["{}{}", hex::to(addr), star]);
            }
            if self.cables.is_empty() {
              ui.write_status("{ no cabals in list }");
            }
            ui.update();
          },
          _ => {},
        }
      },
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
      },
      "/connect" => {
        self.write_status(line).await;
        if self.active_addr.is_none() {
          self.write_status(
            r#"no active cabal to bind this connection. use "/cabal add" first"#
          ).await;
        } else if let Some(tcp_addr) = args.get(1).cloned() {
          let cable = self.get_active_cable().await.unwrap();
          self.connections.insert(Connection::Connected(tcp_addr.clone()));
          let mui = self.ui.clone();
          task::spawn(async move {
            let stream = net::TcpStream::connect(tcp_addr.clone()).await?;
            {
              let mut ui = mui.lock().await;
              ui.write_status(&format!["connected to {}", tcp_addr]);
              ui.update();
            }
            cable.listen(stream).await?;
            let r: Result<(),Error> = Ok(()); r
          });
        } else {
          let mut ui = self.ui.lock().await;
          ui.write_status("usage: /connect HOST:PORT");
          ui.update();
        }
      },
      "/listen" => {
        self.write_status(line).await;
        if self.active_addr.is_none() {
          self.write_status(
            r#"no active cabal to bind this connection. use "/cabal add" first"#
          ).await;
        } else if let Some(mut tcp_addr) = args.get(1).cloned() {
          if !tcp_addr.contains(":") {
            tcp_addr = format!["0.0.0.0:{}", tcp_addr];
          }
          let cable = self.get_active_cable().await.unwrap();
          self.connections.insert(Connection::Listening(tcp_addr.clone()));
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
            let r: Result<(),Error> = Ok(()); r
          });
        } else {
          let mut ui = self.ui.lock().await;
          ui.write_status("usage: /listen (ADDR:)PORT");
          ui.update();
        }
      },
      x => {
        if x.starts_with("/") {
          self.write_status(line).await;
          self.write_status(&format!["no such command: {}", x]).await;
        } else {
          self.post(&line.trim_end().as_bytes()).await?;
        }
      }
    }
    Ok(())
  }
  pub fn add_cable(&mut self, addr: &Addr) {
    let s_addr = hex::to(addr);
    self.cables.insert(addr.to_vec(), Cable::new(*(self.storage_fn)(&s_addr)));
  }
  pub fn set_active_address(&mut self, addr: &Addr) {
    self.active_addr = Some(addr.clone());
  }
  pub async fn post(&mut self, msg: &[u8]) -> Result<(),Error> {
    let mut ui = self.ui.lock().await;
    let w = ui.get_active_window();
    if w.channel == "!status".as_bytes().to_vec() {
      ui.write_status(
        "can't post text in status channel. see /help for command list"
      );
      ui.update();
    } else {
      let cable = self.cables.get_mut(&w.address).unwrap();
      cable.post_text(&w.channel, msg).await?;
    }
    Ok(())
  }
  pub async fn get_active_cable(&mut self) -> Option<Cable<S>> {
    self.active_addr.as_ref().and_then(|addr| self.cables.get(addr).cloned())
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

use async_std::{prelude::*,task,net,sync::{Arc,Mutex}};
use std::collections::{HashMap,HashSet};
use cable::{Cable,Store,Error};
use crate::ui::{UI,Addr,Channel,TermSize};
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
    if line.starts_with("/") {
      let mut ui = self.ui.lock().await;
      ui.write_status(line);
      ui.update();
    }
    let args = line.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
    if args.is_empty() { return Ok(()) }
    match args.get(0).unwrap().as_str() {
      "/help" => {
        let mut ui = self.ui.lock().await;
        ui.write_status("available commands: TODO");
        ui.update();
      },
      "/quit" | "/exit" | "/q" => {
        self.exit = true;
      },
      "/win" | "/w" => {
        let i: usize = args.get(1).unwrap().parse().unwrap();
        let mut ui = self.ui.lock().await;
        ui.set_active_index(i);
        ui.update();
      },
      "/join" | "/j" => {
        let mut ui = self.ui.lock().await;
        if let Some(address) = &self.active_addr {
          if let Some(channel) = args.get(0) {
            ui.add_window(address.clone(), channel.as_bytes().to_vec());
          } else {
            ui.write_status("usage: /join CHANNEL");
          }
        } else {
          ui.write_status(&format!["{}{}",
            "cannot join channel with no active cabal set.",
            " add a cabal with \"/cabal add\" first",
          ]);
        }
        ui.update();
      },
      "/cabal" => {
        match (args.get(1).map(|x| x.as_str()), args.get(2)) {
          (Some("add"),Some(s_addr)) => {
            let addr = from_hex(s_addr);
            self.add_cable(&addr);
            self.write_status(&format!["added cabal: {}", s_addr]).await;
            if self.active_addr.is_none() {
              self.set_active_address(&addr);
              self.write_status(&format!["set active cabal to {}", s_addr]).await;
            }
            self.update().await;
          },
          (Some("list"),_) => {
            let mut ui = self.ui.lock().await;
            for addr in self.cables.keys() {
              let star = if self.active_addr.as_ref().map(|x| x == addr).unwrap_or(false) { "*" } else { "" };
              ui.write_status(&(String::from_utf8(addr.to_vec()).unwrap() + star));
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
        if self.active_addr.is_none() {
          self.write_status(
            r#"no active cabal to bind this connection. use "/cabal add" first"#
          ).await;
        } else if let Some(tcp_addr) = args.get(1).cloned() {
          let cable = self.get_active_cable().await.unwrap().clone();
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
        if self.active_addr.is_none() {
          self.write_status(
            r#"no active cabal to bind this connection. use "/cabal add" first"#
          ).await;
        } else if let Some(mut tcp_addr) = args.get(1).cloned() {
          if !tcp_addr.contains(":") {
            tcp_addr = format!["0.0.0.0:{}", tcp_addr];
          }
          let cable = self.get_active_cable().await.unwrap().clone();
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
          let mut ui = self.ui.lock().await;
          ui.write_status(&format!["no such command: {}", x]);
          ui.update();
        } else {
          self.post(&line.trim_end().as_bytes()).await?;
        }
      }
    }
    Ok(())
  }
  fn add_cable(&mut self, addr: &Addr) {
    let s_addr = String::from_utf8(addr.to_vec()).unwrap();
    self.cables.insert(addr.to_vec(), Cable::new((self.storage_fn)(&s_addr)));
  }
  fn set_active_address(&mut self, addr: &Addr) {
    self.active_addr = Some(addr.clone());
  }
  async fn post(&mut self, msg: &[u8]) -> Result<(),Error> {
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
  async fn get_active_cable<'a>(&'a mut self) -> Option<&'a mut Cable<S>> {
    self.active_addr.as_ref().and_then(|addr| self.cables.get_mut(addr))
  }
  async fn write_status(&self, msg: &str) {
    let mut ui = self.ui.lock().await;
    ui.write_status(msg);
    ui.update();
  }
  async fn update(&self) {
    self.ui.lock().await.update();
  }
}

fn from_hex(s: &str) -> Vec<u8> {
  s.chars().map(|c| u8::from_str_radix(&c.to_string(), 16).unwrap()).collect()
}

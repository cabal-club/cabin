use async_std::{prelude::*,task,net};
use std::collections::HashMap;
use cable::{Cable,Store,ChannelOptions,Error};
use crate::ui::{UI,Addr,Channel};

pub struct App<S: Store> {
  cables: HashMap<Addr,Cable<S>>,
  storage_fn: Box<dyn Fn (&str) -> Box<S>>,
  ui: UI,
}

impl<S> App<S> where S: Store {
  pub fn new(storage_fn: Box<dyn Fn (&str) -> Box<S>>) -> Self {
    Self {
      cables: HashMap::new(),
      storage_fn,
      ui: UI::default(),
    }
  }
  pub async fn handle(&mut self, line: &str) -> Result<(),Error> {
    let args = line.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
    if args.is_empty() { return Ok(()) }
    match args.get(0).unwrap().as_str() {
      "/tcp.connect" => {
        if let Some(addr) = args.get(1).cloned() {
          let cables = self.cables.clone();
          // todo: track connections
          let cable = Cable::new((self.storage_fn)(&addr));
          let ckey = ("tcp+c:".to_string() + &addr).as_bytes().to_vec();
          self.cables.insert(ckey, cable.clone());
          task::spawn(async move {
            let stream = net::TcpStream::connect(addr).await?;
            cable.listen(stream).await?;
            let r: Result<(),Error> = Ok(()); r
          });
        } else {
          self.ui.write_status("usage: /tcp.connect HOST:PORT");
        }
      },
      "/tcp.listen" => {
        if let Some(addr) = args.get(1).cloned() {
          let cable = Cable::new((self.storage_fn)(&addr));
          let ckey = ("tcp+l:".to_string() + &addr).as_bytes().to_vec();
          self.cables.insert(ckey, cable.clone());
          task::spawn(async move {
            let listener = net::TcpListener::bind(addr).await?;
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
          self.ui.write_status("usage: /tcp.listen (ADDR:)PORT");
        }
      },
      x => {
        if x.starts_with("/") {
          self.ui.write_status(&format!["no such command: {}", x]);
        } else {
          self.post(&line.trim_end().as_bytes()).await?;
        }
      }
    }
    Ok(())
  }
  async fn post(&mut self, msg: &[u8]) -> Result<(),Error> {
    if let (addr,channel,Some(cable)) = self.get_active() {
      cable.post_text(&channel, msg).await?;
    } else {
      self.ui.write_status("can't post text in status channel. see /help for command list");
    }
    Ok(())
  }
  fn get_active<'a>(&'a mut self) -> (Addr,Channel,Option<&'a mut Cable<S>>) {
    let (addr,channel) = self.ui.get_active();
    if addr.is_empty() || channel == "!status".as_bytes().to_vec() {
      (addr, channel, None)
    } else {
      let cable = self.cables.get_mut(&addr);
      (addr, channel, cable)
    }
  }
}

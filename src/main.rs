use async_std::{prelude::*,io,task,sync::{Arc,Mutex}};
use cable::MemoryStore;
use std::io::Write;
use signal_hook::{iterator::{SignalsInfo,exfiltrator::WithOrigin},consts::signal::SIGWINCH};
mod app;
mod ui;

type Error = Box<dyn std::error::Error+Send+Sync+'static>;

fn main() -> Result<(),Error> {
  let (args,argv) = argmap::parse(std::env::args());
  task::block_on(async move {
    let mut app = app::App::new(get_size(), Box::new(|_name| {
      Box::new(MemoryStore::default())
    }));
    let ui = app.ui.clone();
    task::spawn(async move { resizer(ui).await });

    let stdin = io::stdin();
    let mut line = String::new();
    loop {
      stdin.read_line(&mut line).await.unwrap();
      app.handle(&line).await.unwrap();
    }
    Ok(())
  })
}

fn get_size() -> ui::TermSize {
  term_size::dimensions().map(|(w,h)| (w as u32, h as u32)).unwrap()
}

async fn resizer(ui: Arc<Mutex<ui::UI>>) {
  let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
  for info in &mut signals {
    if info.signal == SIGWINCH { ui.lock().await.resize(get_size()) }
  }
}

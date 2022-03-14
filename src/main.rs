use async_std::{prelude::*,io,task,sync::{Arc,Mutex}};
use cable::MemoryStore;
use std::io::{Read,Write};
use signal_hook::{iterator::{SignalsInfo,exfiltrator::WithOrigin},consts::signal::SIGWINCH};
use raw_tty::IntoRawMode;
use cabin::{ui::{TermSize,UI},app::App};

type Error = Box<dyn std::error::Error+Send+Sync+'static>;

fn main() -> Result<(),Error> {
  let (args,argv) = argmap::parse(std::env::args());
  task::block_on(async move {
    let mut app = App::new(get_size(), Box::new(|_name| {
      Box::new(MemoryStore::default())
    }));
    let ui = app.ui.clone();
    task::spawn(async move { resizer(ui).await });

    app.run(Box::new(std::io::stdin().into_raw_mode().unwrap())).await?;
    Ok(())
  })
}

fn get_size() -> TermSize {
  term_size::dimensions().map(|(w,h)| (w as u32, h as u32)).unwrap()
}

async fn resizer(ui: Arc<Mutex<UI>>) {
  let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
  for info in &mut signals {
    if info.signal == SIGWINCH { ui.lock().await.resize(get_size()) }
  }
}

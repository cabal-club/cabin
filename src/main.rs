use async_std::{prelude::*,io,task};
use cable::MemoryStore;
mod app;
mod ui;

type Error = Box<dyn std::error::Error+Send+Sync+'static>;

fn main() -> Result<(),Error> {
  let (args,argv) = argmap::parse(std::env::args());

  task::block_on(async move {
    let mut app = app::App::new(Box::new(|_name| {
      Box::new(MemoryStore::default())
    }));
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
      stdin.read_line(&mut line).await.unwrap();
      app.handle(&line).await.unwrap();
    }
    Ok(())
  })
}

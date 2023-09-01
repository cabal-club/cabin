use std::{env, io};

use async_std::task;
use cable::Channel;
use cable_core::MemoryStore;
use futures::channel::mpsc;
use raw_tty::IntoRawMode;

use cabin::{app::App, ui};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

fn main() -> Result<(), Error> {
    // Initialise the logger.
    env_logger::init();

    // Parse the arguments.
    let (_args, _argv) = argmap::parse(env::args());

    // Launch the application, resize the UI to match the terminal dimensions
    // and accept input via stdin.
    task::block_on(async move {
        let (close_channel_sender, close_channel_receiver) = mpsc::unbounded::<Channel>();

        let mut app = App::new(
            ui::get_term_size(),
            Box::new(|_name| Box::<MemoryStore>::default()),
            close_channel_sender,
        );

        let ui = app.ui.clone();
        task::spawn(async move { ui::resizer(ui).await });

        app.run(
            Box::new(io::stdin().into_raw_mode().unwrap()),
            close_channel_receiver,
        )
        .await?;

        Ok(())
    })
}

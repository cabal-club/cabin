use async_std::{
    sync::{Arc, Mutex},
    task,
};
use cabin::{
    app::App,
    ui::{TermSize, UI},
};
use cable::MemoryStore;
use raw_tty::IntoRawMode;
use signal_hook::{
    consts::signal::SIGWINCH,
    iterator::{exfiltrator::WithOrigin, SignalsInfo},
};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

fn main() -> Result<(), Error> {
    let (_args, _argv) = argmap::parse(std::env::args());
    task::block_on(async move {
        let mut app = App::new(
            get_size(),
            Box::new(|_name| Box::new(MemoryStore::default())),
        );
        let ui = app.ui.clone();
        task::spawn(async move { resizer(ui).await });

        app.run(Box::new(std::io::stdin().into_raw_mode().unwrap()))
            .await?;
        Ok(())
    })
}

fn get_size() -> TermSize {
    term_size::dimensions()
        .map(|(w, h)| (w as u32, h as u32))
        .unwrap()
}

async fn resizer(ui: Arc<Mutex<UI>>) {
    let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
    for info in &mut signals {
        if info.signal == SIGWINCH {
            ui.lock().await.resize(get_size())
        }
    }
}

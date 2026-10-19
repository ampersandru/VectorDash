use crate::{
    config::Settings,
    engine::{Event, NotifySender},
};
use anyhow::Result;
use std::{path::Path, sync::OnceLock};
struct Task {
    path: String,
    settings: Settings,
    tx: NotifySender,
}
static QUEUE: OnceLock<crossbeam_channel::Sender<Task>> = OnceLock::new();
pub fn thumbnail(path: String, settings: Settings, tx: NotifySender) -> Result<()> {
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::bounded::<Task>(8);
        for _ in 0..2 {
            let rx = rx.clone();
            std::thread::spawn(move || {
                while let Ok(task) = rx.recv() {
                    let resolved = crate::platform::preview_path(&task.path);
                    let extension = crate::content::extension(&resolved);
                    let result = if crate::content::is_image(&extension)
                        && (!crate::raw::is_raw(&extension) || task.settings.index_raw)
                    {
                        crate::content::decode(&resolved, &task.settings)
                            .and_then(|i| crate::content::jpeg(&i, 128))
                    } else if crate::content::is_video(&extension) {
                        // One frame, only for visible results; no thumbnail files or database blobs.
                        // Keep a small CPU decode independent of an embedding GPU context.
                        let mut settings = task.settings.clone();
                        settings.video_decode_device = "CPU".into();
                        crate::content::video_frame(&resolved, 0.0, &settings)
                            .and_then(|b| image::load_from_memory(&b).map_err(Into::into))
                            .and_then(|i| crate::content::jpeg(&i, 128))
                            .or_else(|_| crate::platform::icon_png(&task.path))
                    } else {
                        crate::platform::icon_png(&task.path)
                    }
                    .map_err(|e| e.to_string());
                    let _ = task.tx.send(Event::Thumbnail(task.path, result));
                }
            });
        }
        tx
    });
    queue
        .try_send(Task { path, settings, tx })
        .map_err(|_| anyhow::anyhow!("Thumbnail queue busy"))
}

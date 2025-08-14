use std::path::PathBuf;

use tokio::sync::mpsc::*;

use anyhow::Result;

#[derive(Debug, Clone)]
pub enum FileDialogKind {
    Save,
    Open,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    MessageDialog {
        description: String,
        title: String,
        level: rfd::MessageLevel,
    },
    FileDialog(FileDialogKind, Sender<PathBuf>),
    StopLoop,
}

pub fn do_dialogs(mut rx: Receiver<Dialog>) -> Result<()> {
    while let Some(dialog) = rx.blocking_recv() {
        match dialog {
            Dialog::MessageDialog {
                level,
                title,
                description,
            } => {
                rfd::MessageDialog::new()
                    .set_level(level)
                    .set_title(title)
                    .set_description(description)
                    .show();
            }
            Dialog::FileDialog(kind, tx) => {
                // TODO: Work around this hack.
                #[cfg(target_os = "windows")]
                std::fs::create_dir_all("C:\\Windows\\system32\\config\\systemprofile\\Desktop")?;

                let dialog = rfd::FileDialog::new().set_directory("/");
                let file = match kind {
                    FileDialogKind::Open => dialog.pick_file(),
                    FileDialogKind::Save => dialog.save_file(),
                };
                if let Some(file) = file {
                    tx.blocking_send(file)?;
                }
            }
            Dialog::StopLoop => break,
        }
    }

    Ok(())
}

pub async fn spawn_file_dialog(
    dialog_tx: &Sender<Dialog>,
    kind: FileDialogKind,
) -> Option<PathBuf> {
    let (file_tx, mut file_rx) = channel(1);
    // this should never fail
    dialog_tx
        .send(Dialog::FileDialog(kind, file_tx))
        .await
        .unwrap();
    file_rx.recv().await
}

pub async fn spawn_message_dialog(
    dialog_tx: &Sender<Dialog>,
    title: impl Into<String>,
    description: impl Into<String>,
    level: rfd::MessageLevel,
) {
    dialog_tx
        .send(Dialog::MessageDialog {
            title: title.into(),
            description: description.into(),
            level,
        })
        .await
        .unwrap();
}

#![windows_subsystem = "windows"]

use std::path::Path;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

const SYS_DIR: &str = r"C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy";
const EXE_NAME: &str = "ScreenClippingHost.exe";

fn show_error(msg: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Erro")
        .set_description(msg)
        .show();
}

fn main() {
    let target = Path::new(SYS_DIR).join(EXE_NAME);
    let backup = Path::new(SYS_DIR).join(format!("{}.backup", EXE_NAME));

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => { show_error(&format!("Não foi possível obter o caminho deste exe:\n{}", e)); return; }
    };
    let source = current_exe.parent().unwrap().join(EXE_NAME);

    if !source.exists() {
        show_error(&format!(
            "Arquivo substituto não encontrado.\n\nColoque o novo {} na mesma pasta deste programa:\n{}",
            EXE_NAME, source.display()
        ));
        return;
    }

    let msg = format!(
        "As seguintes operações serão realizadas:\n\n\
        1. Renomear original:\n   {} → {}.backup\n\n\
        2. Copiar substituto:\n   {} → {}\n\nDeseja continuar?",
        target.display(), EXE_NAME,
        source.display(), target.display()
    );

    let r = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Substituir ScreenClippingHost.exe")
        .set_description(&msg)
        .set_buttons(MessageButtons::YesNo)
        .show();

    if let MessageDialogResult::Yes = r {
        if target.exists() {
            if let Err(e) = std::fs::rename(&target, &backup) {
                show_error(&format!("Erro ao renomear o original:\n{}\n\nTente rodar como Administrador.", e));
                return;
            }
        }
        if let Err(e) = std::fs::copy(&source, &target) {
            let _ = std::fs::rename(&backup, &target);
            show_error(&format!("Erro ao copiar o substituto:\n{}", e));
            return;
        }
        MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title("Concluído")
            .set_description("Substituição feita com sucesso!\nOriginal salvo como .backup.")
            .show();
    }
}

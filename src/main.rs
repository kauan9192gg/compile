#![windows_subsystem = "windows"]

use std::path::Path;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use winreg::enums::*;
use winreg::RegKey;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE, REG_DWORD,
    REG_OPTION_BACKUP_RESTORE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const SYS_DIR: &str = r"C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy";
const EXE_NAME: &str = "ScreenClippingHost.exe";

fn show_error(msg: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Erro")
        .set_description(msg)
        .show();
}

fn show_ok(msg: &str) {
    MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("Concluído")
        .set_description(msg)
        .show();
}

fn instalar_screen_clipping() {
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
        show_ok("Substituição feita com sucesso!\nOriginal salvo como .backup.");
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Liga um privilégio no token do processo (vem desligado por padrão, mesmo como admin).
unsafe fn ativar_privilegio(nome: &str) -> bool {
    let mut token: HANDLE = 0;
    if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
        return false;
    }
    let mut luid = LUID { LowPart: 0, HighPart: 0 };
    let n = wide(nome);
    if LookupPrivilegeValueW(std::ptr::null(), n.as_ptr(), &mut luid) == 0 {
        CloseHandle(token);
        return false;
    }
    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
    };
    let ok = AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) != 0
        && GetLastError() == 0; // 1300 = ERROR_NOT_ALL_ASSIGNED (não tinha o privilégio)
    CloseHandle(token);
    ok
}

/// Grava HKLM\...\Control Panel\Settings\Network\ReplaceVan ignorando o dono (TrustedInstaller),
/// usando os privilégios de Backup/Restauração do administrador.
fn gravar_replace_van(valor: u32) -> Result<(), String> {
    unsafe {
        if !(ativar_privilegio("SeBackupPrivilege") && ativar_privilegio("SeRestorePrivilege")) {
            return Err("Não consegui ativar os privilégios de Backup/Restauração.\nRode o programa como Administrador.".into());
        }
        let caminho = wide(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Control Panel\Settings\Network");
        let mut hkey: HKEY = 0;
        let rc = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            caminho.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_BACKUP_RESTORE,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if rc != 0 {
            return Err(format!("Não consegui abrir a chave (erro {}).", rc));
        }
        let nome = wide("ReplaceVan");
        let dados = valor.to_le_bytes();
        let rc = RegSetValueExW(hkey, nome.as_ptr(), 0, REG_DWORD, dados.as_ptr(), 4);
        RegCloseKey(hkey);
        if rc != 0 {
            return Err(format!("Não consegui gravar o valor (erro {}).", rc));
        }
        Ok(())
    }
}

fn ativar_rede_classica() {
    let r = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Ativar painel de rede clássico")
        .set_description(
            "Ativa o painel de Wi-Fi estilo antigo (Windows 7/8),\n\
            onde a lista de redes abre numa janelinha em vez do app Configurações.\n\n\
            Vai gravar ReplaceVan = 2 no Registro.\n\
            Reinicie o computador depois para aplicar.\n\n\
            Deseja continuar?"
        )
        .set_buttons(MessageButtons::YesNo)
        .show();

    if let MessageDialogResult::Yes = r {
        match gravar_replace_van(2) {
            Ok(())  => show_ok("Painel de rede clássico ativado!\n\nReinicie o Windows para aplicar."),
            Err(e)  => show_error(&format!("Erro ao modificar o Registro:\n{}", e)),
        }
    }
}

fn ativar_volume_classico() {
    let r = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Ativar controle de volume clássico")
        .set_description(
            "Ativa o controle de volume estilo antigo (Windows 7/8),\n\
            o slider pequenininho que aparecia do lado do relógio\n\
            em vez do painel moderno do Windows 10/11.\n\n\
            Reinicie o computador depois para aplicar.\n\n\
            Deseja continuar?"
        )
        .set_buttons(MessageButtons::YesNo)
        .show();

    if let MessageDialogResult::Yes = r {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        match hklm.create_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\MTCUVC") {
            Ok((key, _)) => {
                match key.set_value("EnableMtcUvc", &0u32) {
                    Ok(_)  => show_ok("Controle de volume clássico ativado!\n\nReinicie o Windows para aplicar."),
                    Err(e) => show_error(&format!("Erro ao definir o valor no Registro:\n{}", e)),
                }
            }
            Err(e) => show_error(&format!("Erro ao modificar o Registro:\n{}\n\nTente rodar como Administrador.", e)),
        }
    }
}

fn main() {
    // Menu principal: ScreenClipping vs Tweaks do sistema
    let escolha = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("ScreenFix Tools")
        .set_description(
            "O que deseja fazer?\n\n\
            ◀ ScreenClippingHost → substitui o exe do sistema\n\
            ▶ Tweaks do Sistema  → rede clássica, volume clássico..."
        )
        .set_buttons(MessageButtons::OkCancelCustom(
            "ScreenClippingHost".into(),
            "Tweaks do Sistema".into(),
        ))
        .show();

    match escolha {
        MessageDialogResult::Ok => instalar_screen_clipping(),
        MessageDialogResult::Cancel => {
            // Submenu de tweaks
            let tweak = MessageDialog::new()
                .set_level(MessageLevel::Info)
                .set_title("Tweaks do Sistema")
                .set_description(
                    "Qual tweak deseja aplicar?\n\n\
                    ◀ Rede Clássica   → painel de Wi-Fi estilo Windows 7/8\n\
                    ▶ Volume Clássico → slider de volume estilo Windows 7/8"
                )
                .set_buttons(MessageButtons::OkCancelCustom(
                    "Rede Clássica".into(),
                    "Volume Clássico".into(),
                ))
                .show();

            match tweak {
                MessageDialogResult::Ok     => ativar_rede_classica(),
                MessageDialogResult::Cancel => ativar_volume_classico(),
                _ => {}
            }
        }
        _ => {}
    }
}

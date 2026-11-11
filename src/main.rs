#![windows_subsystem = "windows"]

use std::path::Path;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
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

// ---------------------------------------------------------------- Registro

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

/// Grava um DWORD em HKLM\<caminho>\<nome> ignorando o dono da chave (TrustedInstaller),
/// usando os privilégios de Backup/Restauração do administrador.
fn gravar_dword(caminho: &str, nome: &str, valor: u32) -> Result<(), String> {
    unsafe {
        if !(ativar_privilegio("SeBackupPrivilege") && ativar_privilegio("SeRestorePrivilege")) {
            return Err("não consegui ativar os privilégios de Backup/Restauração (rode como Administrador)".into());
        }
        let caminho_w = wide(caminho);
        let mut hkey: HKEY = 0;
        let rc = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            caminho_w.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_BACKUP_RESTORE,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if rc != 0 {
            return Err(format!("não consegui abrir a chave (erro {})", rc));
        }
        let nome_w = wide(nome);
        let dados = valor.to_le_bytes();
        let rc = RegSetValueExW(hkey, nome_w.as_ptr(), 0, REG_DWORD, dados.as_ptr(), 4);
        RegCloseKey(hkey);
        if rc != 0 {
            return Err(format!("não consegui gravar o valor (erro {})", rc));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- Ajustes

fn aplicar_rede_classica() -> Result<String, String> {
    gravar_dword(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Control Panel\Settings\Network",
        "ReplaceVan",
        2,
    )?;
    Ok("ativado".into())
}

fn aplicar_volume_classico() -> Result<String, String> {
    gravar_dword(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\MTCUVC",
        "EnableMtcUvc",
        0,
    )?;
    Ok("ativado".into())
}

fn aplicar_screen_clipping() -> Result<String, String> {
    let target = Path::new(SYS_DIR).join(EXE_NAME);
    let backup = Path::new(SYS_DIR).join(format!("{}.backup", EXE_NAME));

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("não consegui achar a pasta deste programa ({})", e))?;
    let source = current_exe.parent().unwrap().join(EXE_NAME);

    if !source.exists() {
        return Err(format!(
            "não achei o {} novo na mesma pasta deste programa ({})",
            EXE_NAME,
            source.display()
        ));
    }

    // Se já existe .backup, ele guarda o ORIGINAL do Windows: nunca mexer nele.
    let backup_ja_existe = backup.exists();
    let mut fez_backup_agora = false;

    if target.exists() {
        if backup_ja_existe {
            // O arquivo atual é um substituto antigo: pode apagar, o original está no .backup
            std::fs::remove_file(&target)
                .map_err(|e| format!("erro ao remover o arquivo atual ({})", e))?;
        } else {
            std::fs::rename(&target, &backup)
                .map_err(|e| format!("erro ao renomear o original ({})", e))?;
            fez_backup_agora = true;
        }
    }

    if let Err(e) = std::fs::copy(&source, &target) {
        // Desfaz: devolve o original pro lugar
        if fez_backup_agora {
            let _ = std::fs::rename(&backup, &target);
        } else if backup_ja_existe {
            let _ = std::fs::copy(&backup, &target);
        }
        return Err(format!("erro ao copiar o substituto ({})", e));
    }

    Ok("substituído (original guardado como .backup)".into())
}

fn reiniciar_explorer() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/f", "/im", "explorer.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = std::process::Command::new("explorer.exe").spawn();
}

// ---------------------------------------------------------------- Programa

fn main() {
    let r = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("ScreenFix Tools")
        .set_description(
            "Ao continuar, este programa vai aplicar tudo isto:\n\n\
            • Painel de rede (Wi-Fi) clássico\n\
            • Controle de volume clássico\n\
            • Substituir o ScreenClippingHost.exe do sistema\n\
            \u{00A0}\u{00A0}(o novo precisa estar na mesma pasta deste programa;\n\
            \u{00A0}\u{00A0}o original é guardado como .backup)\n\n\
            Deseja continuar?",
        )
        .set_buttons(MessageButtons::YesNo)
        .show();

    if !matches!(r, MessageDialogResult::Yes) {
        return;
    }

    let rede = aplicar_rede_classica();
    let volume = aplicar_volume_classico();
    let screen = aplicar_screen_clipping();

    let linha = |nome: &str, res: &Result<String, String>| match res {
        Ok(m) => format!("✔ {}: {}", nome, m),
        Err(e) => format!("✘ {}: {}", nome, e),
    };

    let resumo = format!(
        "{}\n{}\n{}",
        linha("Rede clássica", &rede),
        linha("Volume clássico", &volume),
        linha("ScreenClippingHost", &screen),
    );

    let tudo_ok = rede.is_ok() && volume.is_ok() && screen.is_ok();
    MessageDialog::new()
        .set_level(if tudo_ok { MessageLevel::Info } else { MessageLevel::Warning })
        .set_title("Concluído")
        .set_description(&resumo)
        .show();

    if rede.is_ok() || volume.is_ok() {
        let rr = MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title("Aplicar agora?")
            .set_description(
                "Quer reiniciar o Explorer agora pra aplicar?\n\
                (a tela pisca rapidinho)\n\n\
                Se o volume clássico não mudar, reinicie o computador.",
            )
            .set_buttons(MessageButtons::YesNo)
            .show();
        if matches!(rr, MessageDialogResult::Yes) {
            reiniciar_explorer();
        }
    }
}

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONQUESTION, MB_ICONERROR, MB_ICONINFORMATION, MB_YESNO,
};

const NATIVO: &str = r"C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\ScreenClippingHost.exe";
const DESTINO_DIR: &str = r"C:\Programas";

// ---------- helpers de interface ----------

fn para_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Mostra uma caixa Sim/Não. Retorna true se o usuário clicou em "Sim".
fn perguntar(titulo: &str, texto: &str) -> bool {
    let t = para_wide(texto);
    let tit = para_wide(titulo);
    unsafe {
        let r = MessageBoxW(
            std::ptr::null_mut(),
            t.as_ptr(),
            tit.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        );
        r == IDYES
    }
}

/// Mostra uma caixa só com OK (informação ou erro).
fn avisar(titulo: &str, texto: &str, erro: bool) {
    let t = para_wide(texto);
    let tit = para_wide(titulo);
    let icone = if erro { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe {
        MessageBoxW(std::ptr::null_mut(), t.as_ptr(), tit.as_ptr(), icone);
    }
}

// ---------- helpers de sistema ----------

fn rodar(cmd: &str, args: &[&str]) -> bool {
    match Command::new(cmd).args(args).output() {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

// ---------- programa ----------

fn executar() -> Result<(), String> {
    let exe_atual = env::current_exe().map_err(|e| format!("Nao consegui achar meu proprio caminho: {}", e))?;
    let pasta_atual = exe_atual.parent().ok_or("Nao consegui achar a pasta do programa")?;
    let origem_atalho = pasta_atual.join("atalho-captura.exe");
    let origem_nativo = pasta_atual.join("ScreenClippingHost.exe");

    if !origem_atalho.exists() {
        return Err(
            "Nao achei \"atalho-captura.exe\" na mesma pasta deste programa.\n\
             Coloque os dois arquivos juntos e rode de novo."
                .into(),
        );
    }

    if !origem_nativo.exists() {
        return Err(
            "Nao achei \"ScreenClippingHost.exe\" na mesma pasta deste programa.\n\
             Coloque o arquivo junto com o atalho-captura.exe e rode de novo."
                .into(),
        );
    }

    // confirmação antes de mexer no sistema
    if !perguntar(
        "Configurar atalho de captura",
        "Isso vai:\n\n\
         1) Apagar o ScreenClippingHost.exe original do Windows;\n\
         2) Colocar o ScreenClippingHost.exe desta pasta no lugar dele;\n\
         3) Copiar o atalho-captura.exe para C:\\Programas;\n\
         4) Criar uma tarefa agendada com privilégio máximo.\n\n\
         Deseja continuar?",
    ) {
        avisar("Cancelado", "Nada foi alterado.", false);
        return Ok(());
    }

    fs::create_dir_all(DESTINO_DIR)
        .map_err(|e| format!("Nao consegui criar {}: {}", DESTINO_DIR, e))?;

    // 1. apagar o nativo original
    if Path::new(NATIVO).exists() {
        rodar("takeown", &["/f", NATIVO]);
        rodar("icacls", &[NATIVO, "/grant", "administradores:F"]);

        fs::remove_file(NATIVO).map_err(|e| {
            format!(
                "Nao consegui apagar o ScreenClippingHost original:\n{}\n\
                 (talvez esteja em uso — feche o aplicativo de captura e tente de novo)",
                e
            )
        })?;
    }

    // 2. mover o nosso ScreenClippingHost.exe para o local original
    fs::rename(&origem_nativo, NATIVO)
        .map_err(|e| format!("Erro ao mover o ScreenClippingHost.exe: {}", e))?;

    // 3. copiar o atalho-captura e criar tarefa agendada
    let destino_exe = Path::new(DESTINO_DIR).join("atalho-captura.exe");
    fs::copy(&origem_atalho, &destino_exe)
        .map_err(|e| format!("Erro ao copiar o atalho-captura.exe: {}", e))?;

    rodar("schtasks", &["/delete", "/tn", "AtalhoCaptura", "/f"]);
    let caminho_arg = format!("\"{}\"", destino_exe.display());
    let ok = rodar(
        "schtasks",
        &[
            "/create", "/tn", "AtalhoCaptura", "/tr", &caminho_arg, "/sc", "onlogon", "/rl",
            "highest", "/f",
        ],
    );

    if ok {
        avisar(
            "Concluído",
            "Pronto!\n\nReinicie o PC e teste o Win+Shift+S.",
            false,
        );
        Ok(())
    } else {
        Err("Nao consegui criar a tarefa agendada.\nRode este programa como administrador.".into())
    }
}

fn main() {
    if let Err(msg) = executar() {
        avisar("Erro", &msg, true);
    }
}

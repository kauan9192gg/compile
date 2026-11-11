#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
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
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
        .map_err(|e| format!("não consegui achar a pasta do programa ({})", e))?;
    let source = current_exe.parent().unwrap().join(EXE_NAME);

    if !source.exists() {
        return Err(format!("não achei o {} novo na mesma pasta do programa", EXE_NAME));
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

fn windows_build() -> Result<u32, String> {
    let o = Command::new("powershell")
        .args(["-NoProfile", "-Command", "[Environment]::OSVersion.Version.Build"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("não consegui rodar o PowerShell ({})", e))?;
    String::from_utf8_lossy(&o.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| "não consegui detectar a versão do Windows".to_string())
}

/// Remove o app "Captura e Esboço" (Microsoft.ScreenSketch), igual ao .bat antigo.
/// No Windows 11 (build >= 22000) esse pacote é a própria Ferramenta de Captura,
/// então não apaga nada.
fn remover_captura_esboco() -> Result<String, String> {
    let build = windows_build()?;
    if build >= 22000 {
        return Ok("ignorado: no Windows 11 esse app é a própria Ferramenta de Captura".into());
    }

    let script = "$ErrorActionPreference='Stop'; \
        $p = Get-AppxPackage -AllUsers Microsoft.ScreenSketch; \
        if (-not $p) { Write-Output 'NAOEXISTE'; exit 0 }; \
        $p | Remove-AppxPackage -AllUsers";

    let o = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("não consegui rodar o PowerShell ({})", e))?;

    if o.status.success() {
        if String::from_utf8_lossy(&o.stdout).contains("NAOEXISTE") {
            Ok("já não estava instalado".into())
        } else {
            Ok("removido".into())
        }
    } else {
        let err = String::from_utf8_lossy(&o.stderr);
        let linha = err.lines().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("erro desconhecido");
        Err(linha.chars().take(110).collect())
    }
}

// ---------------------------------------------------------------- Explorer

fn matar_processo(nome: &str) {
    let _ = Command::new("taskkill")
        .args(["/f", "/im", nome])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn explorer_rodando() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq explorer.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("explorer.exe"))
        .unwrap_or(false)
}

/// Reabre o Explorer DEPOIS das alterações. O Windows costuma reabrir sozinho depois
/// de um taskkill; se ele já voltou no meio do processo, fecha de novo pra ele
/// subir já com os ajustes. Se não voltar sozinho, abre manualmente.
fn restaurar_explorer() {
    if explorer_rodando() {
        matar_processo("explorer.exe");
        std::thread::sleep(Duration::from_millis(800));
    }
    for _ in 0..10 {
        if explorer_rodando() {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = Command::new("explorer.exe").spawn();
}

// ---------------------------------------------------------------- Execução

struct Resultado {
    rede: Result<String, String>,
    volume: Result<String, String>,
    screen: Result<String, String>,
    captura: Result<String, String>,
}

fn executar() -> Resultado {
    matar_processo("explorer.exe");
    matar_processo(EXE_NAME);
    std::thread::sleep(Duration::from_millis(600));

    let rede = aplicar_rede_classica();
    let volume = aplicar_volume_classico();
    let screen = aplicar_screen_clipping();
    let captura = remover_captura_esboco();

    // Sempre restaura o Explorer, mesmo se algo deu errado
    restaurar_explorer();

    Resultado { rede, volume, screen, captura }
}

// ---------------------------------------------------------------- Interface

enum Estado {
    Confirmar,
    Executando,
    Concluido(Resultado),
}

enum Acao {
    Iniciar,
    Fechar,
}

struct App {
    estado: Estado,
    rx: Option<Receiver<Resultado>>,
    centralizado: bool,
}

fn linha_resultado(ui: &mut egui::Ui, nome: &str, r: &Result<String, String>) {
    ui.horizontal_wrapped(|ui| match r {
        Ok(m) if m.starts_with("ignorado") => {
            ui.label(RichText::new("PULADO").color(Color32::from_rgb(230, 190, 90)).strong());
            ui.label(format!("{}: {}", nome, m));
        }
        Ok(m) => {
            ui.label(RichText::new("OK").color(Color32::from_rgb(90, 200, 120)).strong());
            ui.label(format!("{}: {}", nome, m));
        }
        Err(e) => {
            ui.label(RichText::new("ERRO").color(Color32::from_rgb(235, 90, 90)).strong());
            ui.label(format!("{}: {}", nome, e));
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Centraliza a janela na tela (tenta até o tamanho do monitor estar disponível)
        if !self.centralizado {
            if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(cmd);
                self.centralizado = true;
            } else {
                ctx.request_repaint();
            }
        }

        if let Some(rx) = &self.rx {
            if let Ok(res) = rx.try_recv() {
                self.estado = Estado::Concluido(res);
                self.rx = None;
            }
        }
        if matches!(self.estado, Estado::Executando) {
            ctx.request_repaint_after(Duration::from_millis(100));
            // Bloqueia Alt+F4 (e qualquer pedido de fechar) enquanto está aplicando
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        let moldura = egui::Frame::none()
            .fill(Color32::from_rgb(24, 26, 32))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 75, 90)))
            .inner_margin(egui::Margin::same(22.0));

        let mut acao: Option<Acao> = None;
        let estado = &self.estado;

        egui::CentralPanel::default().frame(moldura).show(ctx, |ui| {
            ui.label(RichText::new("ScreenFix Tools").size(22.0).strong());
            ui.add_space(14.0);

            match estado {
                Estado::Confirmar => {
                    ui.label("Ao continuar, vou aplicar tudo isto:");
                    ui.add_space(8.0);
                    ui.label("•  Painel de rede (Wi-Fi) clássico");
                    ui.label("•  Controle de volume clássico");
                    ui.label("•  Substituir o ScreenClippingHost.exe do sistema");
                    ui.label(
                        RichText::new("    (o novo precisa estar na mesma pasta deste programa;\n     o original é guardado como .backup)")
                            .size(12.0)
                            .color(Color32::from_rgb(150, 155, 170)),
                    );
                    ui.label("•  Remover o app Captura e Esboço (só no Windows 10)");
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("O Explorer vai fechar durante o processo e voltar sozinho no final.")
                            .color(Color32::from_rgb(230, 190, 90)),
                    );
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([150.0, 36.0], egui::Button::new("Sim, aplicar")).clicked() {
                            acao = Some(Acao::Iniciar);
                        }
                        if ui.add_sized([110.0, 36.0], egui::Button::new("Cancelar")).clicked() {
                            acao = Some(Acao::Fechar);
                        }
                    });
                }
                Estado::Executando => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Aplicando... a tela pode piscar, não feche esta janela.");
                    });
                }
                Estado::Concluido(res) => {
                    linha_resultado(ui, "Rede clássica", &res.rede);
                    ui.add_space(4.0);
                    linha_resultado(ui, "Volume clássico", &res.volume);
                    ui.add_space(4.0);
                    linha_resultado(ui, "ScreenClippingHost", &res.screen);
                    ui.add_space(4.0);
                    linha_resultado(ui, "Captura e Esboço", &res.captura);
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Explorer reaberto. Se o volume clássico não mudar, reinicie o computador.")
                            .size(12.0)
                            .color(Color32::from_rgb(150, 155, 170)),
                    );
                    ui.add_space(18.0);
                    if ui.add_sized([130.0, 36.0], egui::Button::new("Fechar")).clicked() {
                        acao = Some(Acao::Fechar);
                    }
                }
            }
        });

        match acao {
            Some(Acao::Iniciar) => {
                let (tx, rx) = channel();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let r = executar();
                    let _ = tx.send(r);
                    ctx2.request_repaint();
                });
                self.rx = Some(rx);
                self.estado = Estado::Executando;
            }
            Some(Acao::Fechar) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
    }
}

fn main() -> eframe::Result<()> {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([470.0, 375.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "ScreenFix Tools",
        opcoes,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(App { estado: Estado::Confirmar, rx: None, centralizado: false }))
        }),
    )
}

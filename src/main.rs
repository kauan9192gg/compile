use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

fn main() {
    let resposta = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("Confirmação")
        .set_description("Deseja continuar?")
        .set_buttons(MessageButtons::YesNo)
        .show();

    match resposta {
        MessageDialogResult::Yes => println!("Você escolheu: Sim"),
        _ => println!("Você escolheu: Não"),
    }
}

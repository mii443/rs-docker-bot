use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};

use log::info;

use regex::Regex;
use tokio::time::{sleep_until, Instant};

use crate::{
    docker::{docker_ps, Container},
    Data, Error,
};

use poise::serenity_prelude::{self as serenity, CreateAttachment, EditMessage, Message};

pub async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { .. } => {
            println!("bot ready");
        }

        serenity::FullEvent::Message { new_message } => {
            println!("on_message");
            on_message(ctx, data, new_message).await;
        }

        _ => {}
    }
    Ok(())
}

async fn on_message(ctx: &serenity::Context, data: &Data, message: &Message) {
    let regex = Regex::new("^```(?P<language>[0-9a-zA-Z]*)\n(?P<code>(\n|.)+)\n```$").unwrap();

    let capture = regex.captures(&message.content);

    if let Some(captures) = capture {
        let language = captures.name("language").unwrap().as_str();
        let code = captures.name("code").unwrap().as_str();

        let config = data.config.lock().await.clone();

        let language = config.get_language(&String::from(language));

        if let Some(language) = language {
            let mut message = message
                .reply(&ctx.http, format!("Creating {} container.", language.name))
                .await
                .unwrap();

            let container = Container::from_language(language.clone()).await;
            let file_name = format!("{}.{}", container.name, language.extension.clone());

            message
                .edit(
                    &ctx.http,
                    EditMessage::new().content(format!("Created: {}", container.id)),
                )
                .await
                .unwrap();

            container.upload_file(code, file_name.clone()).await;

            let compile_buf = Arc::new(Mutex::new(String::default()));
            let b = Arc::clone(&compile_buf);

            if let Some((compile_handle, compile_rx)) = container.compile().await {
                let rx_handle = tokio::spawn(async move {
                    while let Ok(Some(msg)) = compile_rx.recv() {
                        print!("{}", msg);
                        *b.lock().unwrap() += &msg.to_string();
                    }
                });

                let (_, _) = tokio::join!(compile_handle, rx_handle);
            }

            let (run_handle, run_code_rx, end_tx) = container.run_code().await;

            let buf = Arc::new(Mutex::new(String::default()));
            let b = Arc::clone(&buf);
            let rx_handle = tokio::spawn(async move {
                while let Ok(Some(msg)) = run_code_rx.recv() {
                    print!("{}", msg);
                    *b.lock().unwrap() += &msg.to_string();
                }
            });

            let timeout = Arc::new(Mutex::new(false));
            let t = Arc::clone(&timeout);
            tokio::spawn(async move {
                sleep_until(Instant::now() + Duration::from_secs(10)).await;
                end_tx.send(()).unwrap();
                *t.lock().unwrap() = true;
            });

            let (_, _) = tokio::join!(run_handle, rx_handle);

            let mut edit_message = EditMessage::new();
            let mut content = if *timeout.lock().unwrap() {
                "Timeout".to_string()
            } else if compile_buf.lock().unwrap().is_empty() {
                format!(
                    "Result\n```\n{}\n```",
                    buf.lock().unwrap().replace("@", "\\@")
                )
            } else {
                format!(
                    "Result\nCompilation log\n```\n{}\n```\nExecution log\n```{}\n```",
                    compile_buf.lock().unwrap().replace("@", "\\@"),
                    buf.lock().unwrap().replace("@", "\\@")
                )
            };

            if content.len() >= 1000 {
                content = "Result log out of length.".to_string();

                edit_message = edit_message.new_attachment(CreateAttachment::bytes(
                    buf.lock().unwrap().as_bytes(),
                    "result_log.txt",
                ));

                if !((*compile_buf.lock().unwrap()).is_empty()) {
                    edit_message = edit_message.new_attachment(CreateAttachment::bytes(
                        compile_buf.lock().unwrap().as_bytes(),
                        "compile_log.txt",
                    ));
                }
            }

            edit_message = edit_message.content(content);

            message.edit(&ctx.http, edit_message).await.unwrap();

            container.stop().await;
        }
    }
}
/*
pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, context: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command_interaction) = interaction.clone() {
            if command_interaction.data.name == "docker" {
                if command_interaction.data.options()[0].name == "ps" {
                    let list = docker_ps().await;

                    let list: Vec<String> = list
                        .into_iter()
                        .filter(|p| p.state.clone().unwrap() == "running")
                        .map(|f| {
                            String::from(format!("{} {}", f.names.unwrap()[0], f.image.unwrap()))
                        })
                        .collect();

                    let list = list.join("\n");

                    command_interaction
                        .create_interaction_response(
                            &context.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new().content(list),
                            ),
                        )
                        .await
                        .unwrap();
                }

                if command_interaction.data.options()[0].name == "help" {}
            }
        }
    }

    async fn ready(&self, context: Context, _: Ready) {
        info!("Ready.");
        let docker_command = CreateApplicationCommand::new("docker")
            .kind(CommandType::ChatInput)
            .description("docker command")
            .add_option(CreateApplicationCommandOption::new(
                CommandOptionType::SubCommand,
                "ps",
                "docker ps",
            ))
            .add_option(CreateApplicationCommandOption::new(
                CommandOptionType::SubCommand,
                "help",
                "docker help command",
            ));

        Command::create_global_application_command(&context.http, docker_command)
            .await
            .unwrap();
    }
}*/

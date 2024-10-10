use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use regex::Regex;
use tokio::time::{sleep_until, Instant};

use crate::{docker::Container, Data, Error};

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
    let regex = Regex::new("^(?P<codeblock>```(?:(?P<language>[^\n]*)\n)?(?P<code>[\\s\\S]+?)\n```)(?:\\s*(?P<paths>(?:(?:/|\\.\\.?/)?(?:[^/\\s]+/)*[^/\\s]+\\s*)+))?$").unwrap();

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
                    *b.lock().unwrap() += &msg.to_string();
                }
            });

            let timeout = Arc::new(Mutex::new(false));
            let t = Arc::clone(&timeout);
            tokio::spawn(async move {
                sleep_until(Instant::now() + Duration::from_secs(30)).await;
                if let Ok(_) = end_tx.send(()) {
                    *t.lock().unwrap() = true;
                }
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

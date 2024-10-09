mod config;
mod docker;
mod event_handler;
mod language;

use std::{collections::HashSet, env, fs::File, io::Read, sync::Arc};

use config::Config;

use event_handler::event_handler;
use poise::{
    serenity_prelude::{self as serenity, futures::lock::Mutex, UserId},
    PrefixFrameworkOptions,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
#[allow(unused)]
type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub config: Arc<Mutex<Config>>,
}

fn load_config() -> Option<Config> {
    if let Ok(mut config_file) = File::open("config.yaml") {
        let mut buf = String::default();
        config_file.read_to_string(&mut buf).unwrap();
        let config = serde_yaml::from_str::<Config>(&buf);

        if let Ok(config) = config {
            Some(config)
        } else {
            None
        }
    } else {
        None
    }
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    env::set_var("RUST_LOG", "error");
    env_logger::init();

    let config = load_config().unwrap();

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .setup({
            let config = config.clone();
            move |_ctx, _ready, _framework| {
                Box::pin(async move {
                    Ok(Data {
                        config: Arc::new(Mutex::new(config)),
                    })
                })
            }
        })
        .options(poise::FrameworkOptions {
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            commands: vec![],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some(config.prefix),
                ..Default::default()
            },
            owners: HashSet::from([UserId::new(config.owner)]),
            ..Default::default()
        })
        .build();

    let client = serenity::ClientBuilder::new(config.token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();

    Ok(())
}

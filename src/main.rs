mod config;
mod docker;
mod event_handler;
mod language;

use std::{env, fs::File, io::Read, sync::Arc};

use config::Config;
use event_handler::Handler;

use poise::{
    serenity_prelude::{self as serenity, futures::lock::Mutex, UserId},
    PrefixFrameworkOptions,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub context: Arc<Mutex<Config>>,
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
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let config = load_config().unwrap();

    let token = &config.token;
    let http = Http::new(token);

    let bot_id = match http.get_current_user().await {
        Ok(bot_id) => bot_id.id,
        Err(why) => panic!("Could not access the bot id: {:?}", why),
    };

    let framework = StandardFramework::new();
    framework.configure(|c| {
        c.with_whitespace(true)
            .on_mention(Some(bot_id))
            .prefix(config.prefix.clone())
    });

    let mut client = Client::builder(
        &token,
        GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES,
    )
    .framework(framework)
    .event_handler(Handler)
    .await
    .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ConfigStorage>(Arc::new(Mutex::new(config)));
    }

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }

    Ok(())
}

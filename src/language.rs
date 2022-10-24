use bollard::{container::Config, service::HostConfig};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Language {
    pub name: String,
    pub code: Vec<String>,
    pub extension: String,
    pub path: String,
    pub run_command: String,
    pub compile_command: Option<String>,
    pub image: String
}

impl Language {
    pub fn get_path(&self, file_name: String) -> String {
        self.path.clone().replace("{file}", &file_name)
    }

    pub fn get_run_command(&self, file_name: String) -> String {
        self.run_command.clone().replace("{file}", &file_name)
    }

    pub fn get_compile_command(&self, file_name: String) -> Option<String> {
        if let Some(compile) = self.compile_command.clone() {
            Some(compile.replace("{file}", &file_name))
        } else {
            None
        }
    }

    pub fn get_container_option(&self) -> Config<&str> {
        Config {
            image: Some(&self.image),
            tty: Some(true),
            cmd: Some(vec!["/bin/sh"]),
            network_disabled: Some(true),
            stop_timeout: Some(30),
            host_config: Some(HostConfig {
                memory: Some(1024 * 1024 * 1024),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
use std::sync::Arc;

use bollard::{
    container::{RemoveContainerOptions, StopContainerOptions},
    Docker,
};
use tokio::sync::Mutex;

use crate::{
    docker::{docker_ps, Container},
    language::Language,
};

pub struct ContainerPool {
    pub containers: Arc<Mutex<Vec<Container>>>,
}

impl ContainerPool {
    pub fn new() -> Self {
        Self {
            containers: Arc::new(Mutex::new(vec![])),
        }
    }

    pub async fn get_container(&mut self, language: Language) -> Container {
        let mut pool = self.containers.lock().await;
        if let Some(i) = pool.iter().position(|container| {
            if let Some(l) = &container.language {
                l.image == language.image
            } else {
                false
            }
        }) {
            println!("Using container from pool");
            let container_pool = self.containers.clone();
            tokio::spawn(async move {
                let container = Container::from_language(language).await;
                container_pool.lock().await.push(container);
                println!("Added contaienr to pool");
            });
            let container = pool[i].clone();
            pool.remove(i);
            container
        } else {
            Container::from_language(language).await
        }
    }

    pub async fn add_container(&mut self, language: Language) {
        println!("Adding container to pool... {}", language.image);
        let container = Container::from_language(language).await;
        self.containers.lock().await.push(container);
    }

    pub async fn cleanup(&mut self) {
        let docker = Docker::connect_with_local_defaults().unwrap();

        let containers = docker_ps().await;
        println!("{}", containers.len());
        for container in &containers {
            if container
                .names
                .clone()
                .unwrap_or_default()
                .iter()
                .find(|name| name.starts_with("/dockerbot-"))
                .is_some()
            {
                println!("Stopping {}", container.id.clone().unwrap());
                let id = container.id.clone().unwrap();
                docker
                    .stop_container(&id, Some(StopContainerOptions { t: 5 }))
                    .await
                    .unwrap();
                docker
                    .remove_container(
                        &id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await
                    .unwrap();
            }
        }
    }
}

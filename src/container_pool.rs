use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{docker::Container, language::Language};

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
            let container = pool[i].clone();
            pool.remove(i);
            container
        } else {
            Container::from_language(language).await
        }
    }

    pub async fn add_container(&mut self, language: Language) {
        self.containers
            .lock()
            .await
            .push(Container::from_language(language).await);
    }
}

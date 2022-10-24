use std::{
    fs::File,
    io::{Read, Write},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use bollard::{
    container::{
        CreateContainerOptions, ListContainersOptions, LogOutput, RemoveContainerOptions,
        UploadToContainerOptions,
    },
    exec::{CreateExecOptions, StartExecResults},
    service::ContainerSummary,
    Docker,
};
use flate2::{write::GzEncoder, Compression};
use futures_util::StreamExt;
use tokio::task::JoinHandle;

use crate::language::Language;

pub async fn docker_ps() -> Vec<ContainerSummary> {
    let docker = Docker::connect_with_local_defaults().unwrap();

    let options = ListContainersOptions::<String> {
        ..Default::default()
    };

    let list = docker.list_containers(Some(options)).await;

    list.unwrap()
}

pub struct Container {
    pub id: String,
    pub name: String,
    pub language: Option<Language>,
}

impl Container {
    pub async fn from_language(language: Language) -> Self {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let config = language.get_container_option();
        let name = format!("dockerbot-{}", uuid::Uuid::new_v4().to_string());

        let id = docker
            .create_container(Some(CreateContainerOptions { name: name.clone() }), config)
            .await
            .unwrap()
            .id;

        Self {
            id,
            name,
            language: Some(language),
        }
    }

    pub async fn stop(&self) {
        let docker = Docker::connect_with_local_defaults().unwrap();

        docker
            .remove_container(
                &self.id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
    }

    pub async fn run_code(&self) -> (JoinHandle<()>, Receiver<Option<LogOutput>>, Sender<()>) {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let language = self.language.clone().unwrap();
        let file_name = format!("{}.{}", self.name, language.extension);

        let exec = docker
            .create_exec(
                &self.id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stdin: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(
                        language
                            .get_run_command(file_name)
                            .split(" ")
                            .collect(),
                    ),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .id;

        let (tx, rx) = mpsc::channel();
        let (end_tx, end_rx) = mpsc::channel::<()>();

        let handle = tokio::spawn(async move {
            if let StartExecResults::Attached { mut output, .. } =
                docker.start_exec(&exec, None).await.unwrap()
            {
                let mut end_flag = false;
                while !end_flag {
                    if let Ok(_) = end_rx.recv_timeout(Duration::from_millis(10)) {
                        break;
                    }
                    if let Ok(res) =
                        tokio::time::timeout(Duration::from_millis(100), output.next()).await
                    {
                        if let Some(Ok(msg)) = res {
                            tx.send(Some(msg)).unwrap();
                        } else {
                            end_flag = true;
                        }
                    }
                }
            } else {
                unreachable!();
            }

            tx.send(None).unwrap();
        });

        (handle, rx, end_tx)
    }

    pub async fn compile(&self) -> Option<(JoinHandle<()>, Receiver<Option<LogOutput>>)> {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let language = self.language.clone().unwrap();
        let file_name = format!("{}.{}", self.name, language.extension);

        if let Some(compile) = language.get_compile_command(file_name.clone()) {
            let exec = docker
                .create_exec(
                    &self.id,
                    CreateExecOptions {
                        attach_stdout: Some(true),
                        attach_stdin: Some(true),
                        attach_stderr: Some(true),
                        cmd: Some(compile.split(" ").collect()),
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
                .id;

            let (tx, rx) = mpsc::channel();

            let handle = tokio::spawn(async move {
                if let StartExecResults::Attached { mut output, .. } =
                    docker.start_exec(&exec, None).await.unwrap()
                {
                    while let Some(Ok(msg)) = output.next().await {
                        tx.send(Some(msg)).unwrap();
                    }
                } else {
                    unreachable!();
                }

                tx.send(None).unwrap();
            });

            return Some((handle, rx));
        }
        return None;
    }

    pub async fn upload_file(&self, content: &str, file_name: String) {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let path = self.language.clone().unwrap().get_path(file_name.clone());

        {
            let mut f = File::create(&format!("code/{}", path)).unwrap();
            f.write_all(content.as_bytes()).unwrap();
            f.flush().unwrap();
            f.sync_all().unwrap();
        }

        {
            let mut f = File::open(&format!("code/{}", path)).unwrap();

            let tar_gz = File::create(&format!("code/{}.tar.gz", file_name)).unwrap();
            let encoder = GzEncoder::new(tar_gz, Compression::default());
            let mut tar = tar::Builder::new(encoder);
            tar.append_file(
                path.clone(),
                &mut f,
            )
            .unwrap();
            tar.finish().unwrap();
        }

        {
            let mut f = File::open(&format!("code/{}.tar.gz", file_name)).unwrap();
            let mut contents = Vec::new();
            f.read_to_end(&mut contents).unwrap();

            docker
                .start_container::<String>(&self.id, None)
                .await
                .unwrap();

            docker
                .upload_to_container(
                    &self.id,
                    Some(UploadToContainerOptions {
                        path: "/",
                        ..Default::default()
                    }),
                    contents.into(),
                )
                .await
                .unwrap();
        }
    }
}

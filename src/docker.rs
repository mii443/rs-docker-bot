use std::{
    io::Read,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use anyhow::{Context, Result};
use bollard::{
    container::{
        CreateContainerOptions, DownloadFromContainerOptions, ListContainersOptions, LogOutput,
        RemoveContainerOptions, UploadToContainerOptions,
    },
    exec::{CreateExecOptions, StartExecResults},
    service::ContainerSummary,
    Docker,
};
use flate2::{write::GzEncoder, Compression};
use futures_util::StreamExt;
use tar::{Archive, Header};
use tokio::task::JoinHandle;

use crate::language::Language;

#[allow(unused)]
pub async fn docker_ps() -> Vec<ContainerSummary> {
    let docker = Docker::connect_with_local_defaults().unwrap();

    let options = ListContainersOptions::<String> {
        ..Default::default()
    };

    let list = docker.list_containers(Some(options)).await;

    list.unwrap()
}

#[derive(Clone, Debug)]
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
            .create_container(
                Some(CreateContainerOptions {
                    name: name.clone(),
                    platform: None,
                }),
                config,
            )
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
                    cmd: Some(language.get_run_command(file_name).split(" ").collect()),
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

    pub async fn download_file(&self, path: &str) -> Result<Vec<u8>> {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let options = Some(DownloadFromContainerOptions { path });
        let mut download = docker.download_from_container(&self.id, options);
        let mut result: Vec<u8> = vec![];
        while let Some(Ok(d)) = download.next().await {
            result.append(&mut d.into());
        }

        Self::extract(result)
    }

    fn extract(data: Vec<u8>) -> Result<Vec<u8>> {
        let mut archive = Archive::new(data.as_slice());
        let mut entry = archive
            .entries()?
            .next()
            .context("File not found")?
            .context("File not found.")?;
        let mut result = vec![];
        entry.read_to_end(&mut result)?;

        Ok(result)
    }

    pub async fn upload_file(&self, data: Vec<u8>, path: &str) {
        let docker = Docker::connect_with_local_defaults().unwrap();

        let archive = {
            let encoder = GzEncoder::new(vec![], Compression::default());
            let mut tar = tar::Builder::new(encoder);

            let mut header = Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_size(data.len() as u64);
            header.set_cksum();

            tar.append(&header, data.as_slice()).unwrap();

            tar.finish().unwrap();

            let encoder = tar.into_inner().unwrap();
            encoder.finish().unwrap()
        };

        {
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
                    archive.into(),
                )
                .await
                .unwrap();
        }
    }

    pub async fn upload_source_file(&self, content: &str, file_name: String) {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let path = self.language.clone().unwrap().get_path(file_name.clone());

        let code = {
            let encoder = GzEncoder::new(vec![], Compression::default());
            let mut tar = tar::Builder::new(encoder);

            let mut header = Header::new_gnu();
            header.set_path(&path).unwrap();
            header.set_size(content.as_bytes().len() as u64);
            header.set_cksum();

            tar.append(&header, content.as_bytes()).unwrap();

            tar.finish().unwrap();

            let encoder = tar.into_inner().unwrap();
            encoder.finish().unwrap()
        };

        {
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
                    code.into(),
                )
                .await
                .unwrap();
        }
    }
}

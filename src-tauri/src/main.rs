// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use futures_util::{SinkExt, StreamExt};
use nix::libc::{ioctl, winsize, TIOCSWINSZ};
use nix::pty::{openpty, OpenptyResult};
use nix::unistd::{close, dup2, fork, read, setsid, write, ForkResult};
use serde::{Deserialize, Serialize};
use std::os::unix::io::OwnedFd;
use std::os::unix::prelude::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

#[derive(Serialize, Deserialize)]
struct ResizeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    cols: u16,
    rows: u16,
}

fn set_terminal_size(fd: i32, cols: u16, rows: u16) -> nix::Result<()> {
    let ws = winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let res = unsafe { ioctl(fd, TIOCSWINSZ, &ws) };
    if res == 0 {
        Ok(())
    } else {
        Err(nix::Error::last())
    }
}

async fn handle_connection(
    master_fd: OwnedFd,
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) {
    let (mut ws_sender, mut ws_receiver) = stream.split();
    let master_fd = Arc::new(Mutex::new(master_fd));

    let master_fd_clone = master_fd.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            let fd = master_fd_clone.lock().unwrap().as_raw_fd();
            let n = match read(fd, &mut buf) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Error reading from PTY: {}", e);
                    break;
                }
            };
            if n == 0 {
                break;
            }
            let output = String::from_utf8_lossy(&buf[..n]).to_string();
            if ws_sender
                .send(tokio_tungstenite::tungstenite::Message::Text(output))
                .await
                .is_err()
            {
                eprintln!("Error sending to WebSocket");
                break;
            }
        }
    });

    println!("WebSocket connection established");
    while let Some(Ok(msg)) = ws_receiver.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            println!("Received message: {}", text);
            if text.starts_with('{') {
                if let Ok(resize_msg) = serde_json::from_str::<ResizeMessage>(&text) {
                    if resize_msg.msg_type == "resize" {
                        let fd = master_fd.lock().unwrap().as_raw_fd();
                        set_terminal_size(fd, resize_msg.cols, resize_msg.rows)
                            .expect("Resize failed");
                    }
                }
            } else {
                for byte in text.as_bytes() {
                    if let Err(e) = write(&*master_fd.lock().unwrap(), &[*byte]) {
                        eprintln!("Error writing to PTY: {}", e);
                    }
                }
            }
        }
    }
}

#[tauri::command]
async fn start_terminal_server() {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("Listening on: 127.0.0.1:8080");
    while let Ok((stream, _)) = listener.accept().await {
        let ws_stream = accept_async(stream).await.expect("Failed to accept");
        let OpenptyResult { master, slave } = openpty(None, None).unwrap();
        println!(
            "PTY created with master fd: {:?} and slave fd: {:?}",
            master, slave
        );

        match unsafe { fork() }.unwrap() {
            ForkResult::Parent { .. } => {
                close(slave.as_raw_fd()).unwrap();
                tokio::spawn(handle_connection(master, ws_stream));
            }
            ForkResult::Child => {
                setsid().unwrap();
                dup2(slave.as_raw_fd(), 0).unwrap();
                dup2(slave.as_raw_fd(), 1).unwrap();
                dup2(slave.as_raw_fd(), 2).unwrap();
                close(slave.as_raw_fd()).unwrap();

                Command::new("bash").exec();
            }
        }
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            tauri::async_runtime::spawn(async move {
                start_terminal_server().await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

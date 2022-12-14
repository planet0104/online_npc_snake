use bevy::prelude::*;
use snake::*;
use anyhow::Result;

//https://www.jianshu.com/p/34b27a5af889

// fn main() {
//     let mut app = App::new();
    
//     app.add_plugins(HeadlessPlugins)
//     .add_plugin(SnakeGame)
//     .run();
    
// }

use std::env;
use futures_util::{future, StreamExt, TryStreamExt};
use log::info;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<()> {
    let addr = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8080".to_string());

    // 创建我们将接受连接的事件循环和 TCP 侦听器
    let listener = TcpListener::bind(&addr).await?;
    info!("监听地址: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream));
    }

    Ok(())
}

async fn accept_connection(stream: TcpStream) -> Result<()> {
    let addr = stream.peer_addr()?;
    info!("对端地址: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;

    info!("新的 WebSocket 连接: {}", addr);

    let (write, read) = ws_stream.split();
    // 我们不应转发文本或二进制以外的消息.
    read.try_filter(|msg| future::ready(msg.is_text() || msg.is_binary()))
        .forward(write)
        .await?;
    Ok(())
}
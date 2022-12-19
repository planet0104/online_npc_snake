use std::{env, sync::{Arc, Mutex}, collections::HashMap, net::SocketAddr, time::Duration};

use bevy::{prelude::*, app::ScheduleRunnerSettings};
use futures_util::StreamExt;
use snake::*;
use anyhow::Result;
use log::info;

use futures_channel::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};
use futures_util::{future, pin_mut, stream::TryStreamExt};

use tokio::{net::{TcpListener, TcpStream}, runtime::Runtime};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

//https://www.jianshu.com/p/34b27a5af889 使用`thiserror`+`anyhow`来优雅便捷地处理错误

//https://github.com/ElnuDev/bevy-multiplayer/blob/main/src/main.rs bevy websocket例子

#[derive(Resource, Deref, DerefMut)]
pub struct TokioRuntimeHandle(tokio::runtime::Handle);

fn main(){
    App::new()
    .insert_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
        1.0 / 60.0,
    )))
    .add_startup_system(setup_server)
    .add_plugins(HeadlessPlugins)
    .add_plugin(SnakeGame)
    .add_system(receive_message)
    .run();
}

/// 从websocket服务器接收数据
pub fn receive_message(
    _handle: ResMut<TokioRuntimeHandle>,
    mut message_receiver: ResMut<MessageReceiver>,
    message_sender: Res<MessageSender>,
) {
    let msg = match message_receiver.try_next(){
        Ok(Some(v)) => v,
        _ => return
    };
    info!("游戏服务器接收到消息: {:?}", msg);
    let res = message_sender.unbounded_send(IncomingMessage::OnMessage(String::from("I'm Game Server!!")));
    info!("消息已发送给socket服务 {:?}", res);
}

fn setup_server(mut commands: Commands){

    let (sender, receiver) = unbounded::<IncomingMessage>();
    let (sender1, receiver1) = unbounded::<IncomingMessage>();
    commands.insert_resource(MessageReceiver::new(receiver));
    commands.insert_resource(MessageSender::new(sender1));

    let rt  = match Runtime::new(){
        Err(err) => {
            error!("tokio初始化失败: {:?}", err);
            return;
        }
        Ok(v) => v,
    };
    commands.insert_resource(TokioRuntimeHandle(rt.handle().clone()));

    std::thread::spawn(move ||{
        rt.block_on(async {
            match start_server(sender, receiver1).await{
                Ok(()) => info!("websocket服务器结束"),
                Err(err) => error!("websocket服务器出错: {:?}", err)
            };
        });
    });
}

async fn start_server(sender: UnboundedSender<IncomingMessage>, mut receiver: UnboundedReceiver<IncomingMessage>) -> Result<()> {
    let addr = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let state = PeerMap::new(Mutex::new(HashMap::new()));

    // 创建我们将接受连接的事件循环和 TCP 侦听器。
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    // 广播：从游戏服务器发送过来的每一个消息，转发给每一个客户端
    let peer_map = state.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            let peers = peer_map.lock().unwrap();
            
            if let IncomingMessage::OnMessage(msg) = msg{
                let mut broadcast_count = 0;
                for (_addr, recp) in peers.iter() {
                    recp.unbounded_send(Message::Text(msg.clone())).unwrap();
                    broadcast_count += 1;
                }
                info!("给{broadcast_count}个客户端广播了消息: {}", msg);
            }
        }
    });

    // 在单独的任务中生成每个连接的处理
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr, sender.clone()));
    }

    Ok(())
}

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr, sender: UnboundedSender<IncomingMessage>) {
    info!("收到TCP连接: {}", addr);

    let ws_stream = match tokio_tungstenite::accept_async(raw_stream).await{
        Err(err) => {
            error!("websocket握手时出现错误:{:?}", err);
            return;
        }
        Ok(v) => v
    };
    info!("已建立网络套接字连接: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();

    // Sender放入peermap中
    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    // 接收消息的Future
    let broadcast_incoming = incoming.try_for_each(|msg| {
        info!("收到一个消息 {}: {:?}", addr, msg);

        if let Message::Text(msg) = msg{
            info!("消息转发给了游戏服务器: {msg}");
            sender.unbounded_send(IncomingMessage::OnMessage(msg)).unwrap();
        }

        future::ok(())
    });

    // PeerMap中发送的消息，转发到每个客户端的outgoing输出流
    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);

    // 等待两个Future都完成
    future::select(broadcast_incoming, receive_from_others).await;

    info!("{} 连接断开", &addr);
    peer_map.lock().unwrap().remove(&addr);
}
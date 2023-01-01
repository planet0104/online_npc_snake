use std::{env, sync::{Arc, Mutex}, collections::HashMap, net::SocketAddr, time::Duration};

use bevy::{prelude::*, app::ScheduleRunnerSettings};
use futures_util::{StreamExt, SinkExt};
use snake::*;
use anyhow::Result;
use log::info;
use rand::random;
use futures_channel::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};
use futures_util::{future, pin_mut, stream::TryStreamExt};

use tokio::{net::{TcpListener, TcpStream}, runtime::Runtime};
use tungstenite::protocol::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddrWithUUID, Tx>>>;

#[derive(Eq, Hash, PartialEq)]
pub struct SocketAddrWithUUID{
    addr: SocketAddr,
    id: String,
}

impl SocketAddrWithUUID{
    fn new(addr: SocketAddr, id: String) -> Self{
        Self { addr, id }
    }
}

fn main(){
    App::new()
    .insert_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
        1.0 / 60.0,
    )))
    .add_startup_system(setup_server)
    .add_plugins(HeadlessPlugins)
    .add_plugin(SnakeGame)
    .add_system(receive_message)
    .add_system(sync_leader_board)
    .add_system(sync_data.after(snake_movement))
    .run();
}

/// 同步得分榜
pub fn sync_leader_board(
    leader_board: Res<LeaderBoard>,
    message_sender: Res<MessageSender>,
    mut event_reader: EventReader<SyncLeaderBoardEvent>){
    if let Some(_) = event_reader.iter().next(){
        let msg = IncomingMessage::ServerMessage(MessageFromServer::LeaderBoard(leader_board.clone()));
        let _res = message_sender.unbounded_send(msg);
    }
}

/// 给客户端发送同步数据
pub fn sync_data(player_list: Res<PlayerList>,
    snake_positions: Query<&Position, With<SnakeSegment>>,
    mut event_reader: EventReader<SnakeMovementEvent>,
    foods: Query<&Position, With<Food>>,
    message_sender: Res<MessageSender>){
    if let Some(_) = event_reader.iter().next(){
        // 每个玩家，所有实体的坐标点数组
        let mut players = HashMap::new();
        for (id, player_info) in player_list.iter(){
            // 循环玩家蛇头和所有蛇尾的Entity
            let positions = player_info.snake_segments
            .iter()
            // 根据Entity查询到他们的所有Position
            .filter_map(|e| snake_positions.get(*e).ok())
            .map(|pos| *pos)
            .collect::<Vec<Position>>();
            players.insert(id.clone(), positions);
        }
        let foods = foods.iter().map(|v| v.clone()).collect();
        let msg = IncomingMessage::ServerMessage(MessageFromServer::SyncData(SyncData { players, foods }));
        let _res = message_sender.unbounded_send(msg);
    }
}

/// 从websocket服务器接收数据
pub fn receive_message(
    mut message_receiver: ResMut<MessageReceiver>,
    mut commands: Commands,
    mut player_list: ResMut<PlayerList>,
    mut sync_leader_board_writer: EventWriter<SyncLeaderBoardEvent>,
    mut player_heads: Query<(&mut SnakeHead,  &PlayerId)>
) {
    let msg = match message_receiver.try_next(){
        Ok(Some(v)) => v,
        _ => return
    };
    match msg{
        IncomingMessage::ClientMessage(msg) => {
            match msg{
                MessageFromClient::JoinGame((uuid, player_name)) => {
                    //创建玩家，并生成它的蛇
                    let player_info = PlayerInfo {
                        snake_segments: vec![],
                        player_id: uuid.clone(),
                        player_name,
                        spawn_pos: Position::new((random::<f32>() * ARENA_WIDTH as f32) as i32, 0),
                        last_tail_position: None,
                    };
                    player_list.insert(uuid.clone(), player_info);
                    spawn_snake(&mut commands, &mut player_list, uuid, SNAKE_HEAD_COLOR);

                    sync_leader_board_writer.send(SyncLeaderBoardEvent);
                },
                MessageFromClient::LeaveGame(uuid) => {
                    if let Some(player_info) = player_list.remove(&uuid){
                        for seg in player_info.snake_segments{
                            commands.entity(seg).despawn();
                        }
                    }
                    println!("游戏中的玩家数量:{}", player_list.len());
                },
                MessageFromClient::KeyEvent((uuid, key)) =>{
                    for (mut head, player_id) in player_heads.iter_mut(){
                        if player_id.id == uuid{
                            let dir = if key == "L" {
                                snake::Direction::Left
                            } else if key == "D" {
                                snake::Direction::Down
                            } else if key == "U" {
                                snake::Direction::Up
                            } else if key == "R"{
                                snake::Direction::Right
                            } else {
                                head.direction
                            };
                            if dir != head.direction.opposite() {
                                head.direction = dir;
                            }
                            break;
                        }
                    }
                }
                _ => ()
            }
        },
        IncomingMessage::ServerMessage(_) => todo!(),
    }
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
            // info!("需要广播1条消息");
            let peers = peer_map.lock().unwrap();
            
            if let IncomingMessage::ServerMessage(msg) = msg{
                // let mut broadcast_count = 0;
                for (_addr, recp) in peers.iter() {
                    recp.unbounded_send(Message::Binary(bincode::serialize(&msg).unwrap())).unwrap();
                    // broadcast_count += 1;
                }
                // info!("给{broadcast_count}个客户端广播了消息: {:?}", msg);
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
    let id = uuid::Uuid::new_v4().to_string();

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();

    // Sender放入peermap中
    peer_map.lock().unwrap().insert(SocketAddrWithUUID::new(addr, id.clone()), tx);

    let (mut outgoing, incoming) = ws_stream.split();

    // 回复uid
    let msg = MessageFromServer::OnConnected(id.clone());
    outgoing.send(Message::Binary(bincode::serialize(&msg).unwrap())).await.unwrap();

    // 接收消息的Future
    let broadcast_incoming = incoming.try_for_each(|msg| {
        // info!("收到一个消息 {}: {:?}", addr, msg);

        if let Message::Binary(msg) = msg{
            if let Ok(msg) = bincode::deserialize::<MessageFromClient>(&msg){
                // info!("消息转发给了游戏服务器: {:?}", msg);
                sender.unbounded_send(IncomingMessage::ClientMessage(msg)).unwrap();
            }
        }

        future::ok(())
    });

    // PeerMap中发送的消息，转发到每个客户端的outgoing输出流
    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);

    // 等待两个Future都完成
    future::select(broadcast_incoming, receive_from_others).await;

    info!("{} 连接断开", &addr);

    //删除玩家数据
    sender.unbounded_send(IncomingMessage::ClientMessage(MessageFromClient::LeaveGame(id.clone()))).unwrap();

    peer_map.lock().unwrap().remove(&SocketAddrWithUUID { addr, id });
    println!("当前在线玩家:{:?}", peer_map.lock().unwrap().keys().len());
}
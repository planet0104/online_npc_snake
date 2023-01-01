use std::time::Duration;

use bevy::{prelude::*, app::ScheduleRunnerSettings, window::PresentMode, time::FixedTimestep};
// use bevy_inspector_egui::WorldInspectorPlugin;
use futures_channel::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};
use js_sys::Array;
use snake::*;
use anyhow::Result;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{WebSocket, MessageEvent, ErrorEvent};

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
    fn setInterval(closure: &Closure<dyn FnMut()>, millis: u32) -> f64;
    fn clearInterval(token: f64);
}
#[wasm_bindgen(inline_js = "export function open_dialog() { $('#exampleModal').modal('toggle'); }")]
extern "C" {
    fn open_dialog();
}
#[wasm_bindgen(inline_js = "export function close_dialog() { $('#exampleModal').modal('toggle'); }")]
extern "C" {
    fn close_dialog();
}
#[wasm_bindgen(inline_js = "export function set_join_game_callback(cb) { window.joinGame = function(name){  cb(name); }; }")]
extern "C" {
    fn set_join_game_callback(f: &Closure<dyn Fn(String)>);
}
#[wasm_bindgen(inline_js = r#"
    export function update_leader_board(names, scores) {
        updateLeaderBoard(names, scores);
    }
"#)]
extern "C" {
    fn update_leader_board(names: Array, scores: Array);
}

#[wasm_bindgen(start)]
pub fn start() {
    info!("start...");
    if let Err(err) = start_game(){
        error!("游戏启动失败:{:?}", err);
    }
}

/// 当前玩家
#[derive(Resource, Default, Deref, DerefMut)]
pub struct CurrentPlayer(Option<String>);

fn start_game() -> Result<()> {
    info!("start game...");
    // 启动游戏
    App::new()
    .insert_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
        1.0 / 60.0,
    )))
    .add_startup_system(setup_network)
    .insert_resource(ClearColor(Color::rgb(0.04, 0.04, 0.04)))
    .insert_resource(PlayerList::default())
    .insert_resource(CurrentPlayer::default())
    // 窗口设置
    .add_plugins(DefaultPlugins.set(WindowPlugin {
        window: WindowDescriptor {
            title: "贪吃蛇".to_string(),
            width: 720.,
            height: 720.,
            present_mode: PresentMode::AutoVsync,
            ..default()
        },
        ..default()
    }))
    .add_system_set_to_stage(
        CoreStage::PostUpdate,
        SystemSet::new()
            .with_system(position_translation)
            .with_system(size_scaling),
    )
    // .add_plugin(WorldInspectorPlugin::new())
    .add_startup_system(camera_setup)
    .add_system_set(
        SystemSet::new()
            .with_run_criteria(FixedTimestep::step(0.08))
            .with_system(recive_message),
    )
    .add_system(snake_movement_input)
    .run();
    info!("游戏结束...");

    Ok(())
}

pub fn snake_movement_input(
    keyboard_input: Res<Input<KeyCode>>,
    message_sender: Res<MessageSender>,
    current_player: Res<CurrentPlayer>) {
    
    let send_key_msg = |key:&str|{
        if let Some(player) = current_player.0.as_ref(){
            let _ = message_sender.unbounded_send(IncomingMessage::ClientMessage(MessageFromClient::KeyEvent((player.clone(), key.to_string()))));
        }
    };

    if keyboard_input.pressed(KeyCode::Left) {
        send_key_msg("L");
    } else if keyboard_input.pressed(KeyCode::Down) {
        send_key_msg("D");
    } else if keyboard_input.pressed(KeyCode::Up) {
        send_key_msg("U");
    } else if keyboard_input.pressed(KeyCode::Right) {
        send_key_msg("R");
    }
}

fn recive_message(
    mut message_receiver: ResMut<MessageReceiver>,
    message_sender: Res<MessageSender>,
    mut player_list: ResMut<PlayerList>,
    mut current_player: ResMut<CurrentPlayer>,
    mut positions: Query<&mut Position>,
    foods: Query<Entity, With<Food>>,
    mut commands: Commands){
    if let Ok(Some(msg)) = message_receiver.try_next(){
        match msg {
            IncomingMessage::ServerMessage(MessageFromServer::OnConnected(id)) => {
                info!("连接成功! uuid={id}");
                current_player.0.replace(id);
                open_dialog();
            }
            IncomingMessage::ServerMessage(MessageFromServer::LeaderBoard(leader_board)) => {
                info!("得分榜:{:?}", leader_board);
                let names = leader_board.iter().map(|(name, _)| JsValue::from_str(name));
                let scores = leader_board.iter().map(|(_, score)| JsValue::from_f64(*score as f64));
                update_leader_board(js_sys::Array::from_iter(names), js_sys::Array::from_iter(scores));
            }
            IncomingMessage::ClientMessage(MessageFromClient::InputName(user_name)) => {
                if let Some(player_id) = current_player.0.as_ref(){
                    let msg_join = MessageFromClient::JoinGame((player_id.clone(), user_name));
                    let _ = message_sender.unbounded_send(IncomingMessage::ClientMessage(msg_join));
                }
            }
            IncomingMessage::ServerMessage(MessageFromServer::SyncData(mut data)) => {
                // 删除服务器不存在的玩家
                player_list.retain(|k, v|{
                    let contains = data.players.contains_key(k);
                    if !contains{
                        //删除玩家的所有实体
                        for seg in &v.snake_segments{
                            commands.entity(*seg).despawn();
                        }
                    }
                    contains
                });
                // 更新玩家数据
                for (id, player) in data.players{
                    if !player_list.contains_key(&id){
                        //添加玩家
                        let player_id = id.clone();
                        let player_info = PlayerInfo {
                            snake_segments: vec![],
                            player_id:player_id.clone(),
                            player_name: player_id.clone(),
                            spawn_pos: Position::new(0, 0),
                            last_tail_position: None,
                        };
                        player_list.insert(player_id.clone(), player_info);
                        let mut head_color = SNAKE_HEAD_COLOR;
                        if let Some(current_id) = current_player.0.as_ref(){
                            if current_id == &player_id{
                                head_color = SNAKE_HEAD_COLOR_CURRENT;
                            }
                        }
                        spawn_snake(&mut commands, &mut player_list, player_id, head_color);
                    }
                    //检查玩家是否有多余的segment
                    let player_info = player_list.get_mut(&id).unwrap();
                    while player.len() > 0 && player_info.snake_segments.len() > player.len() {
                        let seg = player_info.snake_segments.pop().unwrap();
                        commands.entity(seg).despawn();
                    }
                    
                    for (idx, server_seg_pos) in player.iter().enumerate(){
                        if let Some(client_seg) = player_info.snake_segments.iter_mut().nth(idx){
                            if let Ok(mut pos) = positions.get_mut(*client_seg){
                                *pos = *server_seg_pos;
                            }
                        }else{
                            //长度不够，增加entity
                            player_info.snake_segments.push(spawn_segment(&mut commands, *server_seg_pos));
                        }
                    }
                }
                //删除不存在的Food
                for food in foods.iter(){
                    let pos = match positions.get(food){
                        Err(_) => continue,
                        Ok(pos) => pos
                    };
                    match data.foods.binary_search(pos){
                        Err(_) =>{
                            commands.entity(food).despawn();
                        }
                        Ok(idx) => {
                            // 已存在，不再创建Entity
                            let _ = data.foods.remove(idx);
                        }
                    }
                }
                //添加Food
                for server_pos in data.foods{
                    commands
                    .spawn(SpriteBundle {
                        sprite: Sprite {
                            color: FOOD_COLOR,
                            ..default()
                        },
                        ..default()
                    })
                    .insert(Food)
                    .insert(server_pos)
                    .insert(snake::Size::square(0.8));
                }
            },
            _ => ()
        }
    }
}

fn setup_network(mut commands: Commands){

    let (sender, receiver) = unbounded::<IncomingMessage>();
    let (sender1, receiver1) = unbounded::<IncomingMessage>();
    commands.insert_resource(MessageReceiver::new(receiver));
    commands.insert_resource(MessageSender::new(sender1));

    info!("连接服务器...");
    connect_server(sender, receiver1).unwrap();
}

pub fn connect_server(sender: UnboundedSender<IncomingMessage>, mut receiver: UnboundedReceiver<IncomingMessage>) -> Result<(), JsValue> {
    let ws = WebSocket::new("wss://www.ccfish.run/snake/ws")?;

    //加入游戏
    let sender_clone = sender.clone();
    let closure = Closure::new(move |name:String| {
        info!("加入游戏! {name}");
        if name.trim().len() == 0{
            alert("请输入名字!");
            return;
        }
        info!("name.trim().chars().count()========{}", name.trim().chars().count());
        if name.trim().chars().count() > 10{
            alert("最多输入10个字符!");
            return;
        }
        //关闭对话框
        close_dialog();
        let _ = sender_clone.unbounded_send(IncomingMessage::ClientMessage(MessageFromClient::InputName(name)));
    });
    set_join_game_callback(&closure);
    closure.forget();

    let cloned_ws = ws.clone();

    let closure = Closure::new(move || {
        //检查是否有消息要发送
        if let Ok(Some(msg)) = receiver.try_next(){
            match msg{
                IncomingMessage::ClientMessage(msg) => {
                    //消息发送给服务器端
                    // info!("有消息发送给服务器端:{:?}", msg);
                    let data = bincode::serialize(&msg).unwrap();
                    let _ = cloned_ws.send_with_u8_array(&data);
                },
                _ => ()
            }
        }
    });
    let _token = setInterval(&closure, 10);
    closure.forget();

    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&abuf);
            let data = array.to_vec();
            // info!("接收到二进制数据 长度={}", data.len());
            if let Ok(msg) = bincode::deserialize::<MessageFromServer>(&data){
                // info!("接收到服务器消息:{:?}", msg);
                let _ = sender.unbounded_send(IncomingMessage::ServerMessage(msg));
            }else{
                info!("接收到其他消息:{:?}", data.len());
            }
        }
    });
    // set message event handler on WebSocket
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    // forget the callback to keep it alive
    onmessage_callback.forget();

    let onerror_callback = Closure::<dyn FnMut(_)>::new(move |e: ErrorEvent| {
        info!("error event: {:?}", e);
    });
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    let onopen_callback = Closure::<dyn FnMut()>::new(move || {
        //输入姓名

    });
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    Ok(())
}
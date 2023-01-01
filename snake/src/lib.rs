use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use bevy::{prelude::*, time::{FixedTimestep, TimePlugin}, app::{PluginGroupBuilder, ScheduleRunnerPlugin}, log::LogPlugin};
use futures_channel::mpsc::{UnboundedSender, UnboundedReceiver};
use rand::random;

/// 蛇头颜色
pub const SNAKE_HEAD_COLOR: Color = Color::rgb(0.7, 0.7, 0.7);
pub const SNAKE_HEAD_COLOR_CURRENT: Color = Color::YELLOW;
/// 食物fec938
pub const FOOD_COLOR: Color = Color::rgb(1.0, 0.0, 1.0);
/// 蛇身颜色
const SNAKE_SEGMENT_COLOR: Color = Color::rgb(0.3, 0.3, 0.3);

/// 网格宽度
pub const ARENA_WIDTH: u32 = 40;
/// 网格高度
pub const ARENA_HEIGHT: u32 = 40;

/// 发送给客户端的消息
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum MessageFromServer{
    /// 连接成功, 返回uuid
    OnConnected(String),
    /// 同步玩家列表
    LeaderBoard(LeaderBoard),
    /// 精灵数据
    SyncData(SyncData)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SyncData{
    pub players: HashMap<String, Vec<Position>>,
    pub foods: Vec<Position>,
}

/// 客户端发来的消息
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum MessageFromClient{
    /// 加入游戏 (uuid, user_name)
    JoinGame((String, String)),
    /// 退出游戏(掉线)
    LeaveGame(String),
    KeyEvent((String, String)),
    InputName(String)
}

#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialOrd, PartialEq, Ord, Eq)]
pub struct Position {
    x: i32,
    y: i32,
}

impl Position {
    pub fn new(x: i32, y:i32) -> Self{
        Self{x, y}
    }
}

#[derive(Component)]
pub struct Size {
    width: f32,
    height: f32,
}
impl Size {
    pub fn square(x: f32) -> Self {
        Self {
            width: x,
            height: x,
        }
    }
}

/// 蛇头
#[derive(Component, Debug)]
pub struct SnakeHead{
    pub direction: Direction,
}

#[derive(Component)]
pub struct SnakeSegment;

/// 玩家列表
#[derive(Resource, Default, Deref, DerefMut)]
pub struct PlayerList(HashMap<String, PlayerInfo>);
/// 得分榜
#[derive(Resource, Clone, Serialize, Deserialize, Debug, Default, Deref, DerefMut)]
pub struct LeaderBoard(Vec<(String, usize)>);

#[derive(Debug, Clone)]
pub enum IncomingMessage{
    ClientMessage(MessageFromClient),
    ServerMessage(MessageFromServer)
}

/// 向外部发送消息
#[derive(Resource, Deref, DerefMut)]
pub struct MessageSender(UnboundedSender<IncomingMessage>);
impl MessageSender{
    pub fn new(sender: UnboundedSender<IncomingMessage>) -> Self{
        Self(sender)
    }
}

/// 接收外部消息
#[derive(Resource, Deref, DerefMut)]
pub struct MessageReceiver(UnboundedReceiver<IncomingMessage>);
impl MessageReceiver{
    pub fn new(receiver: UnboundedReceiver<IncomingMessage>) -> Self{
        Self(receiver)
    }
}

pub struct PlayerInfo{
    pub snake_segments: Vec<Entity>,
    pub player_id: String,
    pub player_name: String,
    pub spawn_pos: Position,
    pub last_tail_position: Option<Position>
}

/// 玩家信息
#[derive(Component, Debug)]
pub struct PlayerId{
    pub id: String
}
impl PlayerId{
    pub fn new(id: String) -> Self{
        Self{
            id
        }
    }
}

#[derive(Component)]
pub struct Food;

pub struct GrowthEvent{
    player_id: String
}
pub struct PlayerDeathEvent{
    player_id: String  
}

pub struct SnakeMovementEvent;
pub struct SyncLeaderBoardEvent;

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Direction {
    Left,
    Up,
    Right,
    Down,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

pub fn camera_setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

pub fn game_start(mut _commands: Commands, mut _player_list: ResMut<PlayerList>){
    // info!("游戏开始!");
    
    //添加一个测试玩家
    // let player_id = String::from("NPC Snake");
    // let player_info = PlayerInfo {
    //     snake_segments: vec![],
    //     player_id:player_id.clone(),
    //     player_name: player_id.clone(),
    //     spawn_pos: Position::new(6, 3),
    //     last_tail_position: None,
    // };
    // player_list.insert(player_id.clone(), player_info);

    // spawn_snake(&mut commands, &mut player_list, player_id, SNAKE_HEAD_COLOR);
}

/// 创建小蛇
pub fn spawn_snake(mut commands: &mut Commands, player_list: &mut ResMut<PlayerList>, player_id: String, color: Color) {

    if let Some(player) = player_list.get_mut(&player_id){
        player.snake_segments.clear();
        player.snake_segments.push(commands
            .spawn(SpriteBundle {
                sprite: Sprite {
                    color,
                    ..default()
                },
                ..default()
            })
            .insert(SnakeHead {
                direction: Direction::Up,
            })
            .insert(PlayerId::new(player_id))
            .insert(SnakeSegment)
            .insert(Position::new((random::<f32>() * ARENA_WIDTH as f32) as i32, 0))
            .insert(Size::square(0.8))
            .id());
            
            player.snake_segments.push(spawn_segment(&mut commands, Position::new(player.spawn_pos.x, player.spawn_pos.y-1)));
    }
}

/// 移动蛇
pub fn snake_movement(
    // 查询SnakeSegments数组资源
    mut player_list: ResMut<PlayerList>,
    // 用于发送游戏结束事件
    mut player_death_writer: EventWriter<PlayerDeathEvent>,
    // 发送移动事件
    mut snake_move_event_writer: EventWriter<SnakeMovementEvent>,
    // 查询蛇头实体组件
    heads: Query<(&SnakeHead, &PlayerId)>,
    mut snake_positions: Query<&mut Position, With<SnakeSegment>>
) {

    //所有玩家的蛇头
    for (head, player_id) in heads.iter(){
        let player_info = match player_list.get_mut(&player_id.id){
            None => continue,
            Some(v) => v
        };

        // 循环玩家蛇头和所有蛇尾的Entity
        let segment_positions = player_info.snake_segments
        .iter()
        // 根据Entity查询到他们的所有Position
        .filter_map(|e| snake_positions.get(*e).ok())
        .map(|pos| *pos)
        .collect::<Vec<Position>>();

        //更新玩家的蛇头方向
        let head_entity = *player_info.snake_segments.get(0).unwrap();

        // 获取蛇头实体的位置
        let mut head_pos = match snake_positions.get(head_entity){
            Err(_) => continue,
            Ok(v) => v.clone()
        };

        match &head.direction {
            Direction::Left => {
                head_pos.x -= 1;
            }
            Direction::Right => {
                head_pos.x += 1;
            }
            Direction::Up => {
                head_pos.y += 1;
            }
            Direction::Down => {
                head_pos.y -= 1;
            }
        };

        // 检查蛇头是否碰撞其他蛇、超出屏幕
        if head_pos.x < 0
            || head_pos.y < 0
            || head_pos.x as u32 >= ARENA_WIDTH
            || head_pos.y as u32 >= ARENA_HEIGHT
        {
            player_death_writer.send(PlayerDeathEvent{ player_id: player_info.player_id.clone() });
        }

        for snake_pos in snake_positions.iter(){
            if snake_pos.x == head_pos.x && snake_pos.y == head_pos.y{
                player_death_writer.send(PlayerDeathEvent{ player_id: player_info.player_id.clone() });
                break;
            }
        }

        //更新蛇头位置
        *snake_positions.get_mut(head_entity).unwrap() = head_pos;
        
        // 设置所有蛇身(不包括蛇头)跟随前一个蛇身(包括蛇头)的位置
        segment_positions
        .iter()
        .zip(player_info.snake_segments.iter().skip(1))
        .for_each(|(pos, segment)| {
            *snake_positions.get_mut(*segment).unwrap() = *pos;
        });
        
        // 存储蛇尾的位置
        player_info.last_tail_position = Some(*segment_positions.last().unwrap());
    }

    snake_move_event_writer.send(SnakeMovementEvent);
}

pub fn size_scaling(windows: Res<Windows>, mut q: Query<(&Size, &mut Transform)>) {
    if let Some(window) = windows.get_primary(){
        for (sprite_size, mut transform) in q.iter_mut() {
            transform.scale = Vec3::new(
                sprite_size.width / ARENA_WIDTH as f32 * window.width() as f32,
                sprite_size.height / ARENA_HEIGHT as f32 * window.height() as f32,
                1.0,
            );
        }
    }
}

pub fn position_translation(windows: Res<Windows>, mut q: Query<(&Position, &mut Transform)>) {
    fn convert(pos: f32, bound_window: f32, bound_game: f32) -> f32 {
        let tile_size = bound_window / bound_game;
        pos / bound_game * bound_window - (bound_window / 2.) + (tile_size / 2.)
    }
    if let Some(window) = windows.get_primary(){
        for (pos, mut transform) in q.iter_mut() {
            transform.translation = Vec3::new(
                convert(pos.x as f32, window.width() as f32, ARENA_WIDTH as f32),
                convert(pos.y as f32, window.height() as f32, ARENA_HEIGHT as f32),
                0.0,
            );
        }
    }
}

pub fn food_spawner(mut commands: Commands,
    foods: Query<Entity, With<Food>>,
    positions: Query<&mut Position, With<SnakeSegment>>) {

    // 最多生成20个食物
    if foods.iter().len() >= 20{
        return;
    }

    let mut x = (random::<f32>() * ARENA_WIDTH as f32) as i32;
    let mut y = (random::<f32>() * ARENA_HEIGHT as f32) as i32;

    //禁止在尾巴上生成食物
    loop{
        let collisions = positions.iter().map(|segment_pos|{
                if segment_pos.x == x && segment_pos.y == y{
                    1
                }else{
                    0
                }
            }).collect::<Vec<i32>>().iter().sum::<i32>();

        if collisions == 0{
            break;
        }else{
            //食物位置在蛇尾，重新生成
            x = (random::<f32>() * ARENA_WIDTH as f32) as i32;
            y = (random::<f32>() * ARENA_HEIGHT as f32) as i32;
        }
    }

    commands
        .spawn(SpriteBundle {
            sprite: Sprite {
                color: FOOD_COLOR,
                ..default()
            },
            ..default()
        })
        .insert(Food)
        .insert(Position {x, y})
        .insert(Size::square(0.8));
}

/// 增加蛇身
pub fn spawn_segment(commands: &mut Commands, position: Position) -> Entity {
    commands
        .spawn(SpriteBundle {
            sprite: Sprite {
                color: SNAKE_SEGMENT_COLOR,
                ..default()
            },
            ..default()
        })
        .insert(SnakeSegment)
        .insert(position)
        .insert(Size::square(0.65))
        .id()
}

/// 检测是否有玩家的蛇头吃到了一个食物
pub fn snake_eating(
    mut commands: Commands,
    mut growth_writer: EventWriter<GrowthEvent>,
    food_positions: Query<(Entity, &Position), With<Food>>,
    head_positions: Query<(&PlayerId, &Position), With<SnakeHead>>,
) {
    for(player_id, head_pos) in head_positions.iter(){
        for (ent, food_pos) in food_positions.iter() {
            if food_pos == head_pos {
                // info!("玩家[{:?}]吃到了食物", player_id);
                commands.entity(ent).despawn();
                growth_writer.send(GrowthEvent{ player_id: player_id.id.clone() });
                return;
            }
        }
    }
}

/// 玩家吃到了食物，玩家长大了
pub fn snake_growth(
    mut commands: Commands,
    mut player_segments: ResMut<PlayerList>,
    mut leader_board: ResMut<LeaderBoard>,
    mut sync_leader_board_writer: EventWriter<SyncLeaderBoardEvent>,
    mut growth_reader: EventReader<GrowthEvent>,
) {
    while let Some(event) = growth_reader.iter().next(){
    
        let player_id = &event.player_id;
    
        // info!("玩家[{}]的蛇长大了.", player_id);
    
        if let Some(player_info) = player_segments.get_mut(player_id){
            if let Some(last_tail_position) = player_info.last_tail_position.clone(){
                player_info.snake_segments.push(spawn_segment(&mut commands, last_tail_position));
            }
            //更新得分榜
            let mut found = false;
            for (player_name, score) in leader_board.iter_mut(){
                if player_name == &player_info.player_name{
                    if player_info.snake_segments.len() > * score{
                        *score = player_info.snake_segments.len();
                    }
                    found = true;
                    break;
                }
            }
            if !found{
                leader_board.push((player_info.player_name.clone(), player_info.snake_segments.len()));
            }
            // 排序
            leader_board.sort_by(|(_, score1), (_, score2)| {
                score2.cmp(score1)
            });
            while leader_board.len() > 10{
                let _ = leader_board.pop();
            }
            sync_leader_board_writer.send(SyncLeaderBoardEvent);
        }
    }
}

pub fn player_death(
    mut commands: Commands,
    mut reader: EventReader<PlayerDeathEvent>,
    mut player_list: ResMut<PlayerList>
) {
    let event = match reader.iter().next(){
        None => return,
        Some(event) => event
    };

    let player_id = &event.player_id;

    // info!("玩家[{player_id}]死亡.");

    if let Some(player) = player_list.get(player_id){
        for ent in player.snake_segments.iter(){
            commands.entity(*ent).despawn();
        }
    }

    spawn_snake(&mut commands, &mut player_list, player_id.clone(), SNAKE_HEAD_COLOR);
}

pub struct SnakeGame;

impl Plugin for SnakeGame {
    fn build(&self, app: &mut App) {
        app.add_startup_system(game_start)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(0.15))
                .with_system(snake_movement)
                .with_system(snake_eating.after(snake_movement))
                .with_system(snake_growth.after(snake_eating))
        )
        .add_system(player_death.after(snake_movement))
        
        .insert_resource(ClearColor(Color::rgb(0.04, 0.04, 0.04)))
        .insert_resource(PlayerList::default())
        .insert_resource(LeaderBoard::default())
        .add_event::<GrowthEvent>()
        .add_event::<SnakeMovementEvent>()
        .add_event::<SyncLeaderBoardEvent>()
        .add_event::<PlayerDeathEvent>()
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(2.0))
                .with_system(food_spawner),
        );
    }
}

pub struct HeadlessPlugins;

impl PluginGroup for HeadlessPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
        .add(LogPlugin::default())
        .add(CorePlugin::default())
        .add(TimePlugin::default())
        .add(ScheduleRunnerPlugin::default())
    }
}
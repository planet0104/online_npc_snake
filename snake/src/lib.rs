use std::collections::HashMap;

use bevy::{prelude::*, time::{FixedTimestep, TimePlugin}, app::{PluginGroupBuilder, ScheduleRunnerPlugin}, log::LogPlugin, window::PresentMode};
use bevy_inspector_egui::WorldInspectorPlugin;
use futures_channel::mpsc::{UnboundedSender, UnboundedReceiver};
use rand::random;
//https://bevyengine.org/assets/#assets 教程网站

// https://mbuffett.com/posts/bevy-snake-tutorial/ 贪吃蛇教程

/// 蛇头颜色
const SNAKE_HEAD_COLOR: Color = Color::rgb(0.7, 0.7, 0.7);
/// 食物
const FOOD_COLOR: Color = Color::rgb(1.0, 0.0, 1.0);
/// 蛇身颜色
const SNAKE_SEGMENT_COLOR: Color = Color::rgb(0.3, 0.3, 0.3);

/// 网格宽度
const ARENA_WIDTH: u32 = 20;
/// 网格高度
const ARENA_HEIGHT: u32 = 20;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    x: i32,
    y: i32,
}

impl Position {
    fn new(x: i32, y:i32) -> Self{
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
    direction: Direction,
}

#[derive(Component)]
pub struct SnakeSegment;

// #[derive(Resource, Default, Deref, DerefMut)]
// pub struct SnakeSegments(Vec<Entity>);

/// 玩家列表
#[derive(Resource, Default, Deref, DerefMut)]
pub struct PlayerList(HashMap<String, PlayerInfo>);

#[derive(Debug, Clone)]
pub enum IncomingMessage{
    /// 玩家连线
    OnConnect,
    OnMessage(String),
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
    snake_segments: Vec<Entity>,
    player_id: String,
    spawn_pos: Position,
}

/// 玩家信息
#[derive(Component, Debug)]
pub struct PlayerId(String);

#[derive(Resource, Default)]
pub struct LastTailPosition(Option<Position>);

#[derive(Component)]
pub struct Food;

pub struct GrowthEvent{
    player_id: String
}
pub struct PlayerDeathEvent{
    player_id: String  
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Direction {
    Left,
    Up,
    Right,
    Down,
}

impl Direction {
    fn opposite(self) -> Self {
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

pub fn game_start(mut commands: Commands, mut player_list: ResMut<PlayerList>){
    info!("游戏开始!");
    //添加一个测试玩家
    let player_id = String::from("Planet");
    let player_info = PlayerInfo {
        snake_segments: vec![],
        player_id:player_id.clone(),
        spawn_pos: Position::new(3, 3)
    };
    player_list.insert(player_id.clone(), player_info);

    spawn_snake(&mut commands, &mut player_list, player_id);

    //添加一个测试玩家
    let player_id = String::from("Snake");
    let player_info = PlayerInfo {
        snake_segments: vec![],
        player_id:player_id.clone(),
        spawn_pos: Position::new(6, 3)
    };
    player_list.insert(player_id.clone(), player_info);

    spawn_snake(&mut commands, &mut player_list, player_id);
}

/// 创建小蛇
pub fn spawn_snake(mut commands: &mut Commands, player_list: &mut ResMut<PlayerList>, player_id: String) {

    if let Some(player) = player_list.get_mut(&player_id){
        player.snake_segments.clear();
        player.snake_segments.push(commands
            .spawn(SpriteBundle {
                sprite: Sprite {
                    color: SNAKE_HEAD_COLOR,
                    ..default()
                },
                ..default()
            })
            .insert(SnakeHead {
                direction: Direction::Up,
            })
            .insert(PlayerId(player_id))
            .insert(SnakeSegment)
            .insert(player.spawn_pos.clone())
            .insert(Size::square(0.8))
            .id());
            
            player.snake_segments.push(spawn_segment(&mut commands, Position::new(player.spawn_pos.x, player.spawn_pos.y-1)));
    }
}

pub fn snake_movement_input(keyboard_input: Res<Input<KeyCode>>, mut heads: Query<(&mut SnakeHead,  &PlayerId)>) {
    for (mut head, player_id) in heads.iter_mut(){
        if player_id.0 == "Planet"{
            let dir: Direction = if keyboard_input.pressed(KeyCode::Left) {
                Direction::Left
            } else if keyboard_input.pressed(KeyCode::Down) {
                Direction::Down
            } else if keyboard_input.pressed(KeyCode::Up) {
                Direction::Up
            } else if keyboard_input.pressed(KeyCode::Right) {
                Direction::Right
            } else {
                head.direction
            };
            if dir != head.direction.opposite() {
                head.direction = dir;
            }
            return;
        }
    }
}

/// 移动蛇
pub fn snake_movement(
    // 查询SnakeSegments数组资源
    player_list: Res<PlayerList>,
    mut last_tail_position: ResMut<LastTailPosition>,
    // 用于发送游戏结束事件
    mut player_death_writer: EventWriter<PlayerDeathEvent>,
    // 查询蛇头实体组件
    heads: Query<(&SnakeHead, &PlayerId)>,
    mut snake_positions: Query<&mut Position, With<SnakeSegment>>,
    // 查询所有实体上的Position组件
    // mut positions: Query<&mut Position>,
) {

    //所有玩家的蛇头

    for (head, player_id) in heads.iter(){
        let player_info = match player_list.get(&player_id.0){
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
        *last_tail_position = LastTailPosition(Some(*segment_positions.last().unwrap()));
    }
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
    positions: Query<&mut Position, With<SnakeSegment>>) {

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
                info!("玩家[{:?}]吃到了食物", player_id);
                commands.entity(ent).despawn();
                growth_writer.send(GrowthEvent{ player_id: player_id.0.clone() });
                return;
            }
        }
    }
}

/// 玩家吃到了食物，玩家长大了
pub fn snake_growth(
    mut commands: Commands,
    last_tail_position: Res<LastTailPosition>,
    mut player_segments: ResMut<PlayerList>,
    mut growth_reader: EventReader<GrowthEvent>,
) {
    let event = match growth_reader.iter().next(){
        None => return,
        Some(event) => event
    };

    let player_id = &event.player_id;

    info!("玩家[{}]的蛇长大了.", player_id);

    if let (Some(player_info), Some(last_tail_position)) = (player_segments.get_mut(player_id), last_tail_position.0){
        player_info.snake_segments.push(spawn_segment(&mut commands, last_tail_position));
    }
}

pub fn player_death(
    mut commands: Commands,
    mut reader: EventReader<PlayerDeathEvent>,
    mut player_list: ResMut<PlayerList>,
    // players: ResMut<PlayerList>,
    // food: Query<Entity, With<Food>>,
    // segments: Query<Entity, With<SnakeSegment>>,
) {
    let event = match reader.iter().next(){
        None => return,
        Some(event) => event
    };

    let player_id = &event.player_id;

    info!("玩家[{player_id}]死亡.");

    if let Some(player) = player_list.get(player_id){
        for ent in player.snake_segments.iter(){
            commands.entity(*ent).despawn();
        }
    }

    //清空所有Food
    // for ent in food.iter().chain(segments.iter()) {
    //     commands.entity(ent).despawn();
    // }
    spawn_snake(&mut commands, &mut player_list, player_id.clone());
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
        .insert_resource(LastTailPosition::default())
        .add_event::<GrowthEvent>()
        .add_event::<PlayerDeathEvent>()
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(6.0))
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

pub struct WindowPlugins;

impl Plugin for WindowPlugins {
    fn build(&self, app: &mut App) {
        // 窗口设置
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            window: WindowDescriptor {
                title: "贪吃蛇".to_string(),
                width: 800.,
                height: 800.,
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
        .add_plugin(WorldInspectorPlugin::new())
        .add_startup_system(camera_setup)
        .add_system(snake_movement_input.before(snake_movement));
    }
}
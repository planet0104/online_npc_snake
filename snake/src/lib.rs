use bevy::{prelude::*, time::{FixedTimestep, TimePlugin}, app::{PluginGroupBuilder, ScheduleRunnerPlugin}, log::LogPlugin, window::PresentMode};
use bevy_inspector_egui::WorldInspectorPlugin;
use rand::random;
//https://bevyengine.org/assets/#assets 教程网站

// https://mbuffett.com/posts/bevy-snake-tutorial/ 贪吃蛇教程

/*
结构分析
Position: 组件
Size: 组件

SnakeHead{ direction }：[组件]
SnakeSegment: [组件]
SnakeSegments(Vec<Entity>): [资源] 蛇头、蛇身都存放在这里
LastTailPosition(Option<Position>): [资源] 存放蛇尾的位置

产生一蛇头：spawn SpriteBundle, insert( SnakeHead, SnakeSegment, Position, Size, id )
产生一蛇身：spawn SpriteBundle, insert( SnakeSegment, Position, Size, id )

Food: 组件

产生一个Food: spawn SpriteBundle, insert( Position, Size )

-------------------------------------------------------


 */

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
#[derive(Component)]
pub struct SnakeHead{
    direction: Direction,
}

#[derive(Component)]
pub struct SnakeSegment;

// #[derive(Resource, Default, Deref, DerefMut)]
// pub struct SnakeSegments(Vec<Entity>);

/// 玩家数据
pub struct Player{
    snake_segments: Vec<Entity>,
}

/// 玩家列表
#[derive(Resource, Default, Deref, DerefMut)]
pub struct PlayerList(Vec<Player>);

#[derive(Resource, Default)]
pub struct LastTailPosition(Option<Position>);

#[derive(Component)]
pub struct Food;

pub struct GrowthEvent;
pub struct GameOverEvent;

#[derive(PartialEq, Copy, Clone)]
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

/// 创建小蛇
pub fn spawn_snake(mut commands: Commands, mut segments: ResMut<PlayerList>) {
    segments.iter_mut().for_each(|player| {
        player.snake_segments = vec![
            commands
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
                .insert(SnakeSegment)
                .insert(Position { x: 3, y: 3 })
                .insert(Size::square(0.8))
                .id(),
            spawn_segment(&mut commands, Position { x: 3, y: 2 }),
        ];
    });
}

pub fn snake_movement_input(keyboard_input: Res<Input<KeyCode>>, mut heads: Query<&mut SnakeHead>) {
    if let Some(mut head) = heads.iter_mut().next() {
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
    }
}

/// 移动蛇
pub fn snake_movement(
    // 查询SnakeSegments数组资源
    players: ResMut<PlayerList>,
    mut last_tail_position: ResMut<LastTailPosition>,
    // 用于发送游戏结束事件
    mut game_over_writer: EventWriter<GameOverEvent>,
    // 查询蛇头实体组件
    mut heads: Query<&SnakeHead>,
    // 查询所有实体上的Position组件
    mut positions: Query<&mut Position>,
) {
    players.iter_mut().for_each(|player|{
        //查询到玩家蛇头
        let head_entity = *player.snake_segments.get(0).unwrap();
        let head = heads.get_mut(head_entity).unwrap();
        
        // 循环蛇头和所有蛇尾的Entity
        let segment_positions = player.snake_segments
        .iter()
    // 根据Entity查询到他们的所有Position
        .map(|e| *positions.get_mut(*e).unwrap())
        .collect::<Vec<Position>>();

        // 获取蛇头实体的位置，并增加它的位置
        let mut head_pos = positions.get_mut(head_entity).unwrap();
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

        // 检查蛇头是否碰撞
        if head_pos.x < 0
            || head_pos.y < 0
            || head_pos.x as u32 >= ARENA_WIDTH
            || head_pos.y as u32 >= ARENA_HEIGHT
        {
            game_over_writer.send(GameOverEvent);
        }

        if segment_positions.contains(&head_pos) {
            game_over_writer.send(GameOverEvent);
        }
        
        // 设置所有蛇身(不包括蛇头)跟随前一个蛇身(包括蛇头)的位置
        segment_positions
        .iter()
        .zip(player.snake_segments.iter().skip(1))
        .for_each(|(pos, segment)| {
            *positions.get_mut(*segment).unwrap() = *pos;
        });
        
        // 存储蛇尾的位置
        *last_tail_position = LastTailPosition(Some(*segment_positions.last().unwrap()));
    });
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
    players: Res<PlayerList>,
    mut positions: Query<&mut Position>) {

    let mut x = (random::<f32>() * ARENA_WIDTH as f32) as i32;
    let mut y = (random::<f32>() * ARENA_HEIGHT as f32) as i32;

    //禁止在尾巴上生成食物
    loop{
        let segments = players.iter()
            .filter_map(|player| player.snake_segments.as_ref())
            // .collect::<Vec<&Vec<Entity>>>()
            // .into_iter()
            .flatten()
            .collect::<Vec<&Entity>>();

        let collisions = segments.into_iter()
                .map(|e|{
                let segment_pos = *positions.get_mut(*e).unwrap();
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

pub fn snake_eating(
    mut commands: Commands,
    mut growth_writer: EventWriter<GrowthEvent>,
    food_positions: Query<(Entity, &Position), With<Food>>,
    head_positions: Query<&Position, With<SnakeHead>>,
) {
    for head_pos in head_positions.iter() {
        for (ent, food_pos) in food_positions.iter() {
            if food_pos == head_pos {
                info!("🐍吃到了食物");
                commands.entity(ent).despawn();
                growth_writer.send(GrowthEvent);
            }
        }
    }
}

pub fn snake_growth(
    mut commands: Commands,
    last_tail_position: Res<LastTailPosition>,
    mut segments: ResMut<SnakeSegments>,
    mut growth_reader: EventReader<GrowthEvent>,
) {
    if growth_reader.iter().next().is_some() {
        info!("🐍蛇长大了.");
        segments.push(spawn_segment(&mut commands, last_tail_position.0.unwrap()));
    }
}

pub fn game_over(
    mut commands: Commands,
    mut reader: EventReader<GameOverEvent>,
    segments_res: ResMut<SnakeSegments>,
    food: Query<Entity, With<Food>>,
    segments: Query<Entity, With<SnakeSegment>>,
) {
    if reader.iter().next().is_some() {
        info!("游戏结束.");
        for ent in food.iter().chain(segments.iter()) {
            commands.entity(ent).despawn();
        }
        spawn_snake(commands, segments_res);
    }
}

pub struct SnakeGame;

impl Plugin for SnakeGame {
    fn build(&self, app: &mut App) {
        app.add_startup_system(spawn_snake)
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(0.15))
                .with_system(snake_movement)
                .with_system(snake_eating.after(snake_movement))
                .with_system(snake_growth.after(snake_eating))
        )
        .add_system(game_over.after(snake_movement))
        
        .insert_resource(ClearColor(Color::rgb(0.04, 0.04, 0.04)))
        .insert_resource(SnakeSegments::default())
        .insert_resource(LastTailPosition::default())
        .add_event::<GrowthEvent>()
        .add_event::<GameOverEvent>()
        .add_system_set(
            SystemSet::new()
                .with_run_criteria(FixedTimestep::step(1.0))
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
                width: 400.,
                height: 400.,
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
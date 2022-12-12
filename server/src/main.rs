use bevy::prelude::*;
use snake::*;

fn main() {
    let mut app = App::new();
    
    app.add_plugins(HeadlessPlugins)
    .add_plugin(SnakeGame)
    .run();
}
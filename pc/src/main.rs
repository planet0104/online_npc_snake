use bevy::prelude::*;
use snake::*;

fn main() {
    let mut app = App::new();
    
    app.add_plugin(WindowPlugins)
    .add_plugin(SnakeGame)
    .run();
}
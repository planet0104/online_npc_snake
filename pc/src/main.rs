use std::time::Duration;

use bevy::{prelude::*, app::ScheduleRunnerSettings};
use snake::*;
use tungstenite::{Message, connect};
use url::Url;
use anyhow::Result;

fn main() -> Result<()> {
    // let mut app = App::new();
    
    // app
    // .insert_resource(ScheduleRunnerSettings::run_loop(Duration::from_secs_f64(
    //     1.0 / 60.0,
    // )))
    // .add_plugin(WindowPlugins)
    // .add_plugin(SnakeGame)
    // .run();

    let (mut socket, response) =
        connect(Url::parse("ws://localhost:8080")?)?;

    println!("Connected to the server");
    println!("Response HTTP code: {}", response.status());
    println!("Response contains the following headers:");
    for (ref header, _value) in response.headers() {
        println!("* {}", header);
    }

    socket.write_message(Message::Text("Hello WebSocket".into())).unwrap();
    loop {
        let msg = socket.read_message().expect("Error reading message");
        println!("Received: {}", msg);
    }

    Ok(())
}
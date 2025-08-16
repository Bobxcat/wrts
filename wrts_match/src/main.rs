use std::io::{Read, Write, stderr, stdin, stdout};

use bevy::{
    log::LogPlugin, prelude::*, render::RenderPlugin, window::ExitCondition, winit::WinitPlugin,
};

fn read_io(mut commands: Commands, mut exit: EventWriter<AppExit>, time: Res<Time>) {
    println!("Elapsed seconds: {:?}", time.delta());

    println!("CREATED");
    bevy::log::info!("LOG FROM BEVY");
    exit.write(AppExit::Success);
}

fn main() -> Result<()> {
    let exit = App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                }),
        )
        .add_systems(FixedUpdate, read_io)
        .run();

    println!("exit:{exit:?}");

    let mut buf = vec![0; 13];
    stdin().read_exact(&mut buf)?;
    stdout().write_all(&buf)?;
    // stdout().write_all(b"Hi there! I love")?;
    let s = "Hi there! I lov\n";
    print!("{s}");
    stderr().write_all(b"This is stderr!!")?;

    // println!("Exited: `{:?}`", exit);
    Ok(())
}

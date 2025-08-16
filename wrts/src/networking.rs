use crate::AppState;
use bevy::prelude::*;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::ConnectingToNetwork),
            (setup_connecting_to_network_ui),
        )
        .add_systems(
            Update,
            (update_join_server_button).run_if(in_state(AppState::ConnectingToNetwork)),
        );
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct IPAddressField;

#[derive(Component, Debug, Clone, Copy)]
struct JoinServerButton;

#[derive(Component, Debug, Clone, Copy)]
struct JoinStateDisplay;

fn setup_connecting_to_network_ui(mut commands: Commands) {
    commands.spawn((
        Node {
            display: Display::Flex,
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        StateScoped(AppState::ConnectingToNetwork),
        children![(
            // Node {
            //     flex_direction: FlexDirection::Column,

            // }
        )],
    ));

    let root = commands
        .spawn((
            StateScoped(AppState::ConnectingToNetwork),
            Node {
                display: Display::Flex,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .id();

    let start_button = commands
        .spawn((
            StateScoped(AppState::ConnectingToNetwork),
            JoinServerButton,
            ImageNode::solid_color(Color::linear_rgb(0.2, 0.2, 0.6)),
            Button,
        ))
        .id();

    let quit_button = commands
        .spawn((
            StateScoped(AppState::ConnectingToNetwork),
            JoinStateDisplay,
            ImageNode::solid_color(Color::linear_rgb(0.6, 0.2, 0.2)),
            // Text,
        ))
        .id();

    commands
        .entity(root)
        .add_children(&[start_button, quit_button]);
}

fn update_join_server_button(mut button: Query<&Interaction, With<JoinServerButton>>) {
    let button = button.single_mut().unwrap();
    //
}

// fn

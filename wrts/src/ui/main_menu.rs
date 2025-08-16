use bevy::prelude::*;

use crate::AppState;

pub struct MainMenuUIPlugin;

impl Plugin for MainMenuUIPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<MainMenuUIState>()
            .add_systems(OnEnter(AppState::MainMenu), (setup_main_menu_ui))
            .add_systems(
                Update,
                (update_dim_on_hover, update_start_button, update_quit_button)
                    .run_if(in_state(AppState::MainMenu)),
            );
    }
}

#[derive(SubStates, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[states(scoped_entities)]
#[source(AppState = AppState::MainMenu)]
pub enum MainMenuUIState {
    #[default]
    Main,
}

#[derive(Debug, Component, Clone, Copy)]
struct StartButton;

#[derive(Debug, Component, Clone, Copy)]
struct QuitButton;

#[derive(Debug, Component, Clone, Copy)]
#[require(Interaction, ImageNode)]
struct DimOnHover {
    base_color: Color,
}

fn setup_main_menu_ui(mut commands: Commands) {
    let text_color = Color::linear_rgb(0.2, 0.4, 0.4);

    commands.spawn((
        StateScoped(MainMenuUIState::Main),
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            children![
                (
                    StartButton,
                    Node {
                        margin: UiRect::all(Val::Px(50.0)),
                        ..default()
                    },
                    Text::new("Start"),
                    TextFont {
                        font_size: 67.0,
                        ..default()
                    },
                    TextColor(text_color),
                    ImageNode::solid_color(Color::WHITE),
                    // BackgroundColor(Color::WHITE),
                    Button,
                    DimOnHover {
                        base_color: Color::linear_rgb(0.2, 0.2, 0.6),
                    },
                ),
                (
                    QuitButton,
                    Node {
                        margin: UiRect::all(Val::Px(50.0)),
                        ..default()
                    },
                    Text::new("Quit"),
                    TextFont {
                        font_size: 67.0,
                        ..default()
                    },
                    TextColor(text_color),
                    ImageNode::solid_color(Color::WHITE),
                    // BackgroundColor(Color::WHITE),
                    Button,
                    DimOnHover {
                        base_color: Color::linear_rgb(0.2, 0.2, 0.6),
                    },
                )
            ]
        )],
    ));
}

fn update_dim_on_hover(q: Query<(&DimOnHover, &Interaction, &mut ImageNode)>) {
    for mut item in q {
        let dim = match item.1 {
            Interaction::Pressed => 0.5,
            Interaction::Hovered => 0.8,
            Interaction::None => 1.0,
        };
        item.2.color = Color::LinearRgba((item.0.base_color.to_linear() * dim).with_alpha(1.));
    }
}

fn update_start_button(
    mut button: Query<&Interaction, With<StartButton>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    let button = button.single_mut().unwrap();

    if let Interaction::Pressed = button {
        next_app_state.set(AppState::InGame { paused: true });
    }
}

fn update_quit_button(
    mut button: Query<&Interaction, With<QuitButton>>,
    mut app_exit: EventWriter<AppExit>,
) {
    let button = button.single_mut().unwrap();

    if let Interaction::Pressed = button {
        app_exit.write(AppExit::Success);
    }
}

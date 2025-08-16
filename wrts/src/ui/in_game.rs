use bevy::prelude::*;

use crate::{AppState, InGameState};

pub struct InGameUIPlugin;

impl Plugin for InGameUIPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<InGameUIState>();
        // .add_systems(OnEnter(AppState::InGame), ());
    }
}

#[derive(SubStates, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[states(scoped_entities)]
#[source(InGameState = InGameState)]
pub enum InGameUIState {
    #[default]
    BasicUI,
}

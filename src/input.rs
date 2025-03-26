use bevy::{
    input::gamepad::{
        GamepadAxisChangedEvent, GamepadButtonChangedEvent, GamepadButtonStateChangedEvent,
        GamepadConnection, GamepadConnectionEvent, GamepadEvent, RawGamepadAxisChangedEvent,
        RawGamepadButtonChangedEvent, RawGamepadEvent,
    },
    prelude::*,
};

/// Makes the state of the GameBoy Advance's built in gamepad available using
/// standard Bevy gamepad events.
/// This plugin must be added alongside the [`InputPlugin`](bevy::input::InputPlugin).
#[derive(Default)]
pub struct AgbInputPlugin;

impl Plugin for AgbInputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ButtonController::new())
            .add_event::<GamepadEvent>()
            .add_event::<GamepadConnectionEvent>()
            .add_event::<GamepadButtonChangedEvent>()
            .add_event::<GamepadButtonStateChangedEvent>()
            .add_event::<GamepadAxisChangedEvent>()
            .add_event::<RawGamepadEvent>()
            .add_event::<RawGamepadAxisChangedEvent>()
            .add_event::<RawGamepadButtonChangedEvent>()
            .add_systems(PreUpdate, update_gamepad);
    }

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();

        let gamepad = world.spawn(GameBoyGamepad {}).id();

        let event = GamepadConnectionEvent::new(
            gamepad,
            GamepadConnection::Connected {
                name: "GameBoy Advance Gamepad".to_string(),
                vendor_id: Some(0x057E),
                product_id: None,
            },
        );

        world.send_event::<RawGamepadEvent>(event.clone().into());
        world.send_event::<GamepadConnectionEvent>(event);
    }
}

/// Helper to make it easy to get the current state of the GBA's buttons.
#[derive(Resource, Deref, DerefMut)]
pub struct ButtonController(agb::input::ButtonController);

impl ButtonController {
    #[must_use]
    fn new() -> Self {
        Self(agb::input::ButtonController::new())
    }
}

/// Marker [`Component`] for the [`Entity`] that represents the gamepad built
/// into the GameBoy Advance.
#[derive(Component)]
#[non_exhaustive]
pub struct GameBoyGamepad {}

fn update_gamepad(
    mut manager: ResMut<ButtonController>,
    mut events: EventWriter<RawGamepadEvent>,
    mut button_events: EventWriter<RawGamepadButtonChangedEvent>,
    gamepad: Single<Entity, With<GameBoyGamepad>>,
) {
    manager.update();

    let gamepad = gamepad.into_inner();

    agb::input::Button::all()
        .iter()
        .filter_map(agb_to_bevy_button)
        .filter_map(|(agb_button, bevy_button)| {
            manager
                .is_just_pressed(agb_button)
                .then_some(1.)
                .or(manager.is_just_released(agb_button).then_some(0.))
                .map(|value| RawGamepadButtonChangedEvent::new(gamepad, bevy_button, value))
        })
        .for_each(|event| {
            events.write(event.into());
            button_events.write(event);
        });
}

const fn agb_to_bevy_button(
    button: agb::input::Button,
) -> Option<(agb::input::Button, GamepadButton)> {
    use agb::input::Button;

    match button {
        Button::A => Some((button, GamepadButton::East)),
        Button::B => Some((button, GamepadButton::South)),
        Button::SELECT => Some((button, GamepadButton::Select)),
        Button::START => Some((button, GamepadButton::Start)),
        Button::RIGHT => Some((button, GamepadButton::DPadRight)),
        Button::LEFT => Some((button, GamepadButton::DPadLeft)),
        Button::UP => Some((button, GamepadButton::DPadUp)),
        Button::DOWN => Some((button, GamepadButton::DPadDown)),
        Button::R => Some((button, GamepadButton::RightTrigger)),
        Button::L => Some((button, GamepadButton::LeftTrigger)),
        _ => None,
    }
}

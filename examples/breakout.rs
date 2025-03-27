//! A simplified implementation of the classic game "Breakout".
//!
//! This is based on an example of the same name from Bevy.

#![no_std]
#![no_main]

use agb::display::{object::SpriteLoader, palette16::Palette16};
use bevy::{
    app::PanicHandlerPlugin,
    diagnostic::{DiagnosticsPlugin, FrameCountPlugin},
    input::{
        InputSystem,
        gamepad::{gamepad_connection_system, gamepad_event_processing_system},
    },
    math::{
        bounding::{Aabb2d, BoundingCircle, BoundingVolume, IntersectsVolume},
        ops,
    },
    prelude::*,
    state::app::StatesPlugin,
    time::TimePlugin,
};
use bevy_mod_gba::{AgbSoundPlugin, Sprite, SpriteHandles, Video, prelude::*};

// These constants are defined in `Transform` units.
// Using the default 2D camera they correspond 1:1 with screen pixels.
const PADDLE_SIZE: Vec2 = Vec2::new(32.0, 8.0);
const GAP_BETWEEN_PADDLE_AND_FLOOR: f32 = 4.0;
const PADDLE_SPEED: f32 = 128.0;
// How close can the paddle get to the wall
const PADDLE_PADDING: f32 = 2.0;

// We set the z-value of the ball to 1 so it renders on top in the case of overlapping sprites.
const BALL_STARTING_POSITION: Vec3 = Vec3::new(80.0, 80.0, 1.0);
const BALL_DIAMETER: f32 = 8.;
const BALL_SPEED: f32 = 64.0;
const INITIAL_BALL_DIRECTION: Vec2 = Vec2::new(0.5, 0.5);

const WALL_THICKNESS: f32 = 0.0;
// x coordinates
const LEFT_WALL: f32 = 0.;
const RIGHT_WALL: f32 = 240. - WALL_THICKNESS;
// y coordinates
const BOTTOM_WALL: f32 = 160. - WALL_THICKNESS;
const TOP_WALL: f32 = 0.;

const BRICK_SIZE: Vec2 = Vec2::new(16., 8.);
// These values are exact
const GAP_BETWEEN_PADDLE_AND_BRICKS: f32 = 110.0;
const GAP_BETWEEN_BRICKS: f32 = 4.0;
// These values are lower bounds, as the number of bricks is computed
const GAP_BETWEEN_BRICKS_AND_CEILING: f32 = 4.0;
const GAP_BETWEEN_BRICKS_AND_SIDES: f32 = 4.0;

const SCORE_SPEED_INCREMENT: f32 = 1.;

/// Main entry point.
#[expect(unsafe_code)]
#[unsafe(export_name = "main")]
pub extern "C" fn main() -> ! {
    App::new()
        .add_plugins(AgbPlugin.set(AgbSoundPlugin {
            enable_dmg: true,
            mixer_frequency: Some(agb::sound::mixer::Frequency::Hz10512),
            ..default()
        }))
        .add_plugins((
            PanicHandlerPlugin,
            TaskPoolPlugin::default(),
            FrameCountPlugin,
            TimePlugin,
            TransformPlugin,
            DiagnosticsPlugin,
            StatesPlugin,
        ))
        .add_systems(
            PreUpdate,
            (
                gamepad_connection_system,
                gamepad_event_processing_system.after(gamepad_connection_system),
            )
                .in_set(InputSystem),
        )
        .insert_resource(Score(0))
        .init_non_send_resource::<Option<Sprites>>()
        .add_event::<CollisionEvent>()
        .add_systems(Startup, (setup_video, load_sprites, setup).chain())
        .add_systems(
            Update,
            (
                apply_velocity,
                move_paddle,
                check_for_collisions,
                play_collision_sound,
            )
                .chain(),
        )
        .run();

    loop {}
}

fn setup_video(mut video: ResMut<Video>) {
    let (_background, mut vram) = video.tiled0();

    vram.set_background_palettes(&[Palette16::new([0xFFFF; 16])]);
}

fn load_sprites(
    mut loader: NonSendMut<SpriteLoader>,
    mut handles: NonSendMut<SpriteHandles>,
    mut sprites: NonSendMut<Option<Sprites>>,
) {
    static SQUARE_GRAPHICS: &agb::display::object::Graphics =
        agb::include_aseprite!("./assets/square.aseprite");

    static SQUARE: &agb::display::object::Tag = SQUARE_GRAPHICS.tags().get("Square");

    static PADDLE_GRAPHICS: &agb::display::object::Graphics =
        agb::include_aseprite!("./assets/paddle.aseprite");

    static PADDLE: &agb::display::object::Tag = PADDLE_GRAPHICS.tags().get("Paddle");

    static BRICK_GRAPHICS: &agb::display::object::Graphics =
        agb::include_aseprite!("./assets/brick.aseprite");

    static BRICK: &agb::display::object::Tag = BRICK_GRAPHICS.tags().get("Brick");

    let square = Sprite::new(handles.add(loader.get_vram_sprite(SQUARE.sprite(0))));
    let paddle = Sprite::new(handles.add(loader.get_vram_sprite(PADDLE.sprite(0))));
    let brick = Sprite::new(handles.add(loader.get_vram_sprite(BRICK.sprite(0))));

    *sprites = Some(Sprites {
        square,
        paddle,
        brick,
    });
}

struct Sprites {
    square: Sprite,
    paddle: Sprite,
    brick: Sprite,
}

#[derive(Component)]
struct Paddle;

#[derive(Component)]
struct Ball;

#[derive(Component, Deref, DerefMut)]
struct Velocity(Vec2);

#[derive(Event, Default)]
struct CollisionEvent;

#[derive(Component)]
struct Brick;

// Default must be implemented to define this as a required component for the Wall component below
#[derive(Component)]
struct Collider {
    half_size: Vec2,
}

// This is a collection of the components that define a "Wall" in our game
#[derive(Component)]
#[require(Transform)]
struct Wall;

/// Which side of the arena is this wall located on?
enum WallLocation {
    Left,
    Right,
    Bottom,
    Top,
}

impl WallLocation {
    /// Location of the *center* of the wall, used in `transform.translation()`
    fn position(&self) -> Vec2 {
        match self {
            WallLocation::Left => Vec2::new(LEFT_WALL, 0.),
            WallLocation::Right => Vec2::new(RIGHT_WALL, 0.),
            WallLocation::Bottom => Vec2::new(0., BOTTOM_WALL),
            WallLocation::Top => Vec2::new(0., TOP_WALL),
        }
    }

    /// (x, y) dimensions of the wall, used in `transform.scale()`
    fn size(&self) -> Vec2 {
        let arena_height = BOTTOM_WALL - TOP_WALL;
        let arena_width = RIGHT_WALL - LEFT_WALL;
        // Make sure we haven't messed up our constants
        assert!(arena_height > 0.0);
        assert!(arena_width > 0.0);

        match self {
            WallLocation::Left | WallLocation::Right => {
                Vec2::new(WALL_THICKNESS, arena_height + WALL_THICKNESS)
            }
            WallLocation::Bottom | WallLocation::Top => {
                Vec2::new(arena_width + WALL_THICKNESS, WALL_THICKNESS)
            }
        }
    }
}

impl Wall {
    // This "builder method" allows us to reuse logic across our wall entities,
    // making our code easier to read and less prone to bugs when we change the logic
    // Notice the use of Sprite and Transform alongside Wall, overwriting the default values defined for the required components
    fn new(location: WallLocation) -> (Wall, Transform, Collider) {
        (
            Wall,
            Transform {
                // We need to convert our Vec2 into a Vec3, by giving it a z-coordinate
                // This is used to determine the order of our sprites
                translation: location.position().extend(0.0),
                ..default()
            },
            Collider {
                half_size: location.size() / 2.,
            },
        )
    }
}

// This resource tracks the game's score
#[derive(Resource, Deref, DerefMut)]
struct Score(usize);

#[derive(Component)]
struct ScoreboardUi;

// Add the game's entities to our world
fn setup(mut commands: Commands, sprites: NonSend<Option<Sprites>>) {
    let sprites = sprites.as_ref().unwrap();

    let square = sprites.square.clone();
    let paddle = sprites.paddle.clone();
    let brick = sprites.brick.clone();

    // Paddle
    let paddle_y = BOTTOM_WALL - GAP_BETWEEN_PADDLE_AND_FLOOR - 8.;

    commands.spawn((
        paddle,
        Transform {
            translation: Vec3::new(120., paddle_y, 0.0),
            ..default()
        },
        Paddle,
        Collider {
            half_size: PADDLE_SIZE / 2.,
        },
    ));

    // Ball
    commands.spawn((
        square,
        Transform::from_translation(BALL_STARTING_POSITION),
        Ball,
        Velocity(INITIAL_BALL_DIRECTION.normalize() * BALL_SPEED),
    ));

    // Walls
    commands.spawn(Wall::new(WallLocation::Left));
    commands.spawn(Wall::new(WallLocation::Right));
    commands.spawn(Wall::new(WallLocation::Bottom));
    commands.spawn(Wall::new(WallLocation::Top));

    // Bricks
    let total_width_of_bricks =
        (RIGHT_WALL - LEFT_WALL - WALL_THICKNESS) - 2. * GAP_BETWEEN_BRICKS_AND_SIDES;
    let bottom_edge_of_bricks = paddle_y - GAP_BETWEEN_PADDLE_AND_BRICKS;
    let total_height_of_bricks = TOP_WALL + bottom_edge_of_bricks + GAP_BETWEEN_BRICKS_AND_CEILING;

    assert!(total_width_of_bricks > 0.0);
    assert!(total_height_of_bricks > 0.0);

    // Given the space available, compute how many rows and columns of bricks we can fit
    let n_columns =
        ops::floor(total_width_of_bricks / (BRICK_SIZE.x + GAP_BETWEEN_BRICKS)) as usize;
    let n_rows = ops::floor(total_height_of_bricks / (BRICK_SIZE.y + GAP_BETWEEN_BRICKS)) as usize;
    let n_vertical_gaps = n_columns - 1;

    // Because we need to round the number of columns,
    // the space on the top and sides of the bricks only captures a lower bound, not an exact value
    let center_of_bricks = (LEFT_WALL + WALL_THICKNESS + RIGHT_WALL) / 2.0;
    let left_edge_of_bricks = center_of_bricks
        // Space taken up by the bricks
        - (n_columns as f32 / 2.0 * BRICK_SIZE.x)
        // Space taken up by the gaps
        - n_vertical_gaps as f32 / 2.0 * GAP_BETWEEN_BRICKS;

    // In Bevy, the `translation` of an entity describes the center point,
    // not its bottom-left corner
    let offset_x = left_edge_of_bricks;
    let offset_y = bottom_edge_of_bricks;

    for row in 0..n_rows {
        for column in 0..n_columns {
            let brick_position = Vec2::new(
                offset_x + column as f32 * (BRICK_SIZE.x + GAP_BETWEEN_BRICKS),
                offset_y - row as f32 * (BRICK_SIZE.y + GAP_BETWEEN_BRICKS),
            );

            // brick
            commands.spawn((
                brick.clone(),
                Transform {
                    translation: brick_position.extend(0.0),
                    ..default()
                },
                Brick,
                Collider {
                    half_size: BRICK_SIZE / 2.,
                },
            ));
        }
    }
}

fn move_paddle(
    gamepad: Single<&Gamepad>,
    mut paddle_transform: Single<&mut Transform, With<Paddle>>,
    time: Res<Time>,
) {
    let mut direction = 0.0;

    if gamepad.pressed(GamepadButton::DPadLeft) {
        direction -= 1.0;
    }

    if gamepad.pressed(GamepadButton::DPadRight) {
        direction += 1.0;
    }

    // Calculate the new horizontal paddle position based on player input
    let new_paddle_position =
        paddle_transform.translation.x + direction * PADDLE_SPEED * time.delta_secs();

    // Update the paddle position,
    // making sure it doesn't cause the paddle to leave the arena
    let left_bound = LEFT_WALL + WALL_THICKNESS + PADDLE_PADDING;
    let right_bound = RIGHT_WALL - PADDLE_SIZE.x - PADDLE_PADDING;

    paddle_transform.translation.x = new_paddle_position.clamp(left_bound, right_bound);
}

fn apply_velocity(mut query: Query<(&mut Transform, &Velocity)>, time: Res<Time>) {
    for (mut transform, velocity) in &mut query {
        transform.translation.x += velocity.x * time.delta_secs();
        transform.translation.y += velocity.y * time.delta_secs();
    }
}

fn check_for_collisions(
    mut commands: Commands,
    mut score: ResMut<Score>,
    ball_query: Single<(&mut Velocity, &Transform), With<Ball>>,
    collider_query: Query<(Entity, &Transform, Option<&Brick>, &Collider)>,
    mut collision_events: EventWriter<CollisionEvent>,
) {
    let (mut ball_velocity, ball_transform) = ball_query.into_inner();

    for (collider_entity, collider_transform, maybe_brick, collider) in &collider_query {
        let collision = ball_collision(
            BoundingCircle::new(
                ball_transform.translation.truncate()
                    + Vec2 {
                        x: BALL_DIAMETER / 2.,
                        y: BALL_DIAMETER / 2.,
                    },
                BALL_DIAMETER / 2.,
            ),
            Aabb2d::new(
                collider_transform.translation.truncate() + collider.half_size,
                collider.half_size,
            ),
        );

        if let Some(collision) = collision {
            // Writes a collision event so that other systems can react to the collision
            collision_events.write_default();

            // Bricks should be despawned and increment the scoreboard on collision
            if maybe_brick.is_some() {
                commands.entity(collider_entity).despawn();
                **score += 1;

                ball_velocity.0 += Vec2::splat(SCORE_SPEED_INCREMENT);
            }

            // Reflect the ball's velocity when it collides
            let mut reflect_x = false;
            let mut reflect_y = false;

            // Reflect only if the velocity is in the opposite direction of the collision
            // This prevents the ball from getting stuck inside the bar
            match collision {
                Collision::Left => reflect_x = ball_velocity.x > 0.0,
                Collision::Right => reflect_x = ball_velocity.x < 0.0,
                Collision::Top => reflect_y = ball_velocity.y > 0.0,
                Collision::Bottom => reflect_y = ball_velocity.y < 0.0,
            }

            // Reflect velocity on the x-axis if we hit something on the x-axis
            if reflect_x {
                ball_velocity.x = -ball_velocity.x;
            }

            // Reflect velocity on the y-axis if we hit something on the y-axis
            if reflect_y {
                ball_velocity.y = -ball_velocity.y;
            }
        }
    }
}

fn play_collision_sound(
    mut collision_events: EventReader<CollisionEvent>,
    mut mixer: NonSendMut<agb::sound::mixer::Mixer>,
) {
    static COLLISION_SOUND: &[u8] = agb::include_wav!("assets/sounds/breakout_collision.wav");

    if !collision_events.is_empty() {
        let sound_channel = agb::sound::mixer::SoundChannel::new(COLLISION_SOUND);
        mixer.play_sound(sound_channel);
    }

    collision_events.clear();
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Collision {
    Left,
    Right,
    Top,
    Bottom,
}

// Returns `Some` if `ball` collides with `bounding_box`.
// The returned `Collision` is the side of `bounding_box` that `ball` hit.
fn ball_collision(ball: BoundingCircle, bounding_box: Aabb2d) -> Option<Collision> {
    if !ball.intersects(&bounding_box) {
        return None;
    }

    let closest = bounding_box.closest_point(ball.center());
    let offset = ball.center() - closest;
    let side = if offset.x.abs() > offset.y.abs() {
        if offset.x < 0. {
            Collision::Left
        } else {
            Collision::Right
        }
    } else if offset.y < 0. {
        Collision::Top
    } else {
        Collision::Bottom
    };

    Some(side)
}

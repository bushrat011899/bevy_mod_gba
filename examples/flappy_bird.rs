//! An implementation of the game Flappy Bird.
//!
//! This is based on an example of the same name from Bevy.

#![no_std]
#![no_main]

//! [`agb`] provides a global allocator, allowing us to use items from the [`alloc`] crate.
extern crate alloc;

use agb::{
    display::{
        HEIGHT,
        object::SpriteLoader,
        tiled::{MapLoan, RegularBackgroundSize, RegularMap, TileFormat, TiledMap, VRamManager},
    },
    include_background_gfx,
    rng::RandomNumberGenerator,
};

use bevy::{
    app::PanicHandlerPlugin,
    diagnostic::{DiagnosticsPlugin, FrameCount, FrameCountPlugin},
    input::{
        InputSystem,
        gamepad::{gamepad_connection_system, gamepad_event_processing_system},
    },
    prelude::*,
    state::app::StatesPlugin,
    time::TimePlugin,
};
use bevy_math::{Rect, Vec2};
use bevy_mod_gba::{AgbSoundPlugin, Sprite, SpriteHandles, Video, prelude::*};
use log::info;

const FLAP_FORCE: f32 = 500. / 10.;
const GRAVITY: f32 = 2000. / 10.;
const OBSTACLE_AMOUNT: i32 = 3;

#[derive(Resource)]
struct BevyRng {
    rng: RandomNumberGenerator,
}

#[derive(Component, PartialEq)]
enum GameObjectType {
    Obstacle,
    TopPipe,
    BotPipe,
}

#[derive(Component)]
struct Obstacle;

#[derive(Component)]
struct Pipe;

#[derive(Component)]
struct PipePart;

struct PipeSprites {
    big_top: Sprite,
}

struct Sprites {
    player: Sprite,
    pipe_sprites: PipeSprites,
}

#[derive(Component, Debug)]
struct Player {
    velocity: f32,
    dead: bool,
    paused: bool,
}

/// Main entry point.
#[expect(unsafe_code)]
#[unsafe(export_name = "main")]
pub extern "C" fn main() -> ! {
    let mut app = App::new();
    app.add_plugins(AgbPlugin.set(AgbSoundPlugin {
        enable_dmg: true,
        ..default()
    }));
    app.add_plugins((
        PanicHandlerPlugin,
        TaskPoolPlugin::default(),
        FrameCountPlugin,
        TimePlugin,
        TransformPlugin,
        DiagnosticsPlugin,
        StatesPlugin,
    ));
    app.add_systems(
        PreUpdate,
        (
            gamepad_connection_system,
            gamepad_event_processing_system.after(gamepad_connection_system),
        )
            .in_set(InputSystem),
    );

    app.init_non_send_resource::<Option<Sprites>>();
    app.add_systems(Startup, (init_setup_video, init_load_sprites).chain());
    app.add_systems(Startup, init_spawn_player.after(init_load_sprites));
    app.add_systems(Startup, init_spawn_pipes.after(init_load_sprites));
    app.add_systems(Startup, init_rng);

    app.add_systems(
        FixedUpdate,
        (
            update_game_start,
            update_bird_move.in_set(TransformSystem::TransformPropagate),
            update_pipe_move.in_set(TransformSystem::TransformPropagate),
            update_collision_check,
            update_reset_handler,
        ),
    );

    app.run();

    loop {}
}

fn init_load_sprites(
    mut sprite_loader: NonSendMut<SpriteLoader>,
    mut sprite_handles: NonSendMut<SpriteHandles>,
    mut sprites: NonSendMut<Option<Sprites>>,
) {
    static BIRD_SPRITES: &agb::display::object::Graphics =
        agb::include_aseprite!("./assets/flappy_bird/bird.aseprite");
    static PIPE_SPRITES: &agb::display::object::Graphics =
        agb::include_aseprite!("./assets/flappy_bird/pipe.aseprite");

    let bird_idle_vram =
        sprite_loader.get_vram_sprite(BIRD_SPRITES.tags().get("bird_idle").sprite(0));
    let bird_idle_handle = sprite_handles.add(bird_idle_vram);

    let big_pipe_top_vram =
        sprite_loader.get_vram_sprite(PIPE_SPRITES.tags().get("pipe_top").sprite(0));
    let big_pipe_top_handle = sprite_handles.add(big_pipe_top_vram);

    *sprites = Some(Sprites {
        player: Sprite::new(bird_idle_handle.clone()),
        pipe_sprites: PipeSprites {
            big_top: Sprite::new(big_pipe_top_handle),
        },
    });
}

fn set_background(mut vram: &mut VRamManager, bg: &mut MapLoan<'_, RegularMap>) {
    include_background_gfx!(backgrounds, "000000",
        background => deduplicate "assets/flappy_bird/background.aseprite",
    );
    vram.set_background_palettes(backgrounds::PALETTES);
    let bg_tiledata = &backgrounds::background;
    bg.fill_with(&mut vram, bg_tiledata);
    bg.commit(&mut vram);
    bg.set_visible(true);
}

fn init_setup_video(mut video: ResMut<Video>) {
    let (gfx, mut vram) = video.tiled0();
    let mut bg = gfx.background(
        agb::display::Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    set_background(&mut vram, &mut bg);
}

fn init_rng(mut commands: Commands) {
    commands.insert_resource(BevyRng {
        rng: RandomNumberGenerator::new(),
    });
}

fn get_default_pipe_sprites(sprites: &Sprites) -> Vec<(Sprite, Transform)> {
    vec![
        ( sprites.pipe_sprites.big_top.clone(), Transform::from_xyz(0., 0., 0.), ),
        ( sprites.pipe_sprites.big_top.clone(), Transform::from_xyz(0., 64., 0.), ),
    ]
}

fn create_pipe(commands: &mut Commands, sprites: &Sprites, top_or_bot: GameObjectType) -> Entity {
    let mut pipe_sprite_info = get_default_pipe_sprites(sprites);
    pipe_sprite_info[1].0.vertical_flipped = true;
    pipe_sprite_info[1].0.visible = false;
    pipe_sprite_info[0].0.visible = false;
    let y = if top_or_bot == GameObjectType::TopPipe {
        -64.
    } else {
        HEIGHT as f32 - 64.
    };
    let pipe_parent = commands
        .spawn((Transform::from_xyz(0., y, 0.), top_or_bot, Pipe {}))
        .id();
    let pipe_sprites: Vec<Entity> = pipe_sprite_info
        .into_iter()
        .map(|simg| commands.spawn((simg.0, simg.1, PipePart {})).id())
        .collect();
    // add children to pipe
    commands.entity(pipe_parent).add_children(&pipe_sprites);

    pipe_parent
}

fn spawn_obstacle(mut commands: &mut Commands, sprites: &Sprites, x: f32) -> Entity {
    let obstacle_obj = commands
        .spawn((
            GameObjectType::Obstacle,
            Transform::from_xyz(x, 0., 0.),
            Obstacle {},
        ))
        .id();

    let parts = vec![
        create_pipe(&mut commands, sprites, GameObjectType::TopPipe),
        create_pipe(&mut commands, sprites, GameObjectType::BotPipe),
    ];

    // add pipe to obstacle
    commands.entity(obstacle_obj).add_children(&parts);
    obstacle_obj
}

fn init_spawn_pipes(mut commands: Commands, sprites: NonSend<Option<Sprites>>) {
    let sprites = sprites.as_ref().unwrap();
    for i in 0..OBSTACLE_AMOUNT {
        spawn_obstacle(
            &mut commands,
            sprites,
            100. + (i as f32 / OBSTACLE_AMOUNT as f32) * (agb::display::WIDTH + 32) as f32,
        );
    }
}

fn init_spawn_player(mut commands: Commands, sprites: NonSend<Option<Sprites>>) {
    let sprites = sprites.as_ref().unwrap();
    let _bird = commands
        .spawn((
            Player {
                velocity: 0.,
                dead: false,
                paused: true,
            },
            Transform::from_xyz(40., 40., 0.),
            sprites.player.clone(),
        ))
        .id();
}

fn update_pipe_move(
    mut q_pipes: Query<(&mut Transform, &mut GameObjectType)>,
    mut rm_rng: ResMut<BevyRng>,
    s_player: Single<&Player>,
    r_fc: Res<FrameCount>,
) {
    if !s_player.paused && r_fc.0 % 3 == 0 {
        for (mut transform, objtype) in q_pipes.iter_mut() {
            if objtype.as_ref() == &GameObjectType::Obstacle {
                transform.translation.x -= 1.;
                if transform.translation.x < -32. {
                    transform.translation.x = agb::display::WIDTH as f32;
                    randomize_pipe_height(&mut rm_rng, &mut transform);
                }
            }
        }
    }
}

fn rects_collide(rect_a: &Rect, rect_b: &Rect) -> bool {
    let no_overlap_x = rect_a.max.x < rect_b.min.x || rect_a.min.x > rect_b.max.x;
    let no_overlap_y = rect_a.max.y < rect_b.min.y || rect_a.min.y > rect_b.max.y;
    !(no_overlap_x || no_overlap_y)
}

fn update_collision_check(
    s_player: Single<(&Transform, &mut Player)>,
    q_pipepart_gt: Query<&mut GlobalTransform, (With<PipePart>, Without<Player>)>,
) {
    let (transform, mut bird) = s_player.into_inner();
    let bird_loc = transform.translation;
    let bird_box = Rect {
        min: Vec2 {
            x: bird_loc.x,
            y: bird_loc.y,
        },
        max: Vec2 {
            x: bird_loc.x + 8.,
            y: bird_loc.y + 8.,
        },
    };
    for transform in q_pipepart_gt {
        let loc = transform.translation();
        let pipe_box = Rect {
            min: Vec2 { x: loc.x, y: loc.y },
            max: Vec2 { x: loc.x + 32., y: loc.y + 64., },
        };
        if rects_collide(&bird_box, &pipe_box) {
            bird.dead = true;
        }
    }
    if bird_loc.y < 0. || bird_loc.y - 8. > HEIGHT as f32 {
        bird.dead = true;
    }
}

fn update_bird_move(
    s_bird: Single<(&mut Transform, &mut Player)>,
    s_gamepad: Single<&Gamepad>,
    r_time: Res<Time>,
) {
    let (mut transform, mut s_bird) = s_bird.into_inner();
    if s_gamepad.pressed(GamepadButton::DPadUp) {
        s_bird.velocity = FLAP_FORCE;
    }

    if !s_bird.paused {
        s_bird.velocity -= r_time.delta_secs() * GRAVITY;
        transform.translation.y -= s_bird.velocity * r_time.delta_secs();
    }
}

fn update_game_start(
    s_bird: Single<&mut Player, (Without<Obstacle>, Without<PipePart>)>,
    mut q_pipe_parts: Query<&mut Sprite, (Without<Obstacle>, With<PipePart>)>,
    mut q_obstacle_transforms: Query<&mut Transform, (With<Obstacle>, Without<PipePart>)>,
    mut rm_rng: ResMut<BevyRng>,
    s_gamepad: Single<&Gamepad, (Without<Obstacle>, Without<PipePart>)>,
    r_fc: Res<FrameCount>,
) {
    let mut bird = s_bird.into_inner();
    if bird.paused && s_gamepad.pressed(GamepadButton::DPadUp) {
        // seed rng on round start to get first pipe positions
        rm_rng.as_mut().rng = RandomNumberGenerator::new_with_seed([1, 1, 1, r_fc.0]);
        for mut part in q_pipe_parts.iter_mut() {
            part.as_mut().visible = true;
        }
        for mut t in q_obstacle_transforms.iter_mut() {
            randomize_pipe_height(&mut rm_rng, &mut t);
        }
        bird.paused = false;
    }
}

fn reset_player(bird_loc: &mut Transform, bird_data: &mut Player) {
    bird_loc.translation.x = 40.;
    bird_loc.translation.y = 40.;
    bird_data.dead = false;
    bird_data.velocity = 0.;
}

fn randomize_pipe_height(rm_rng: &mut BevyRng, t: &mut Transform) {
    let rng = &mut rm_rng.rng;
    let z = (rng.r#gen() as i8) / 4;
    t.translation.y = z as f32;
}

fn update_reset_handler(
    mut s_player: Query<
        (&mut Transform, &mut Player),
        (
            With<Player>,
            Without<Obstacle>,
            Without<Pipe>,
            Without<PipePart>,
        ),
    >,
    mut q_pipe_parts: Query<&mut Sprite, (Without<Obstacle>, With<PipePart>)>,
    mut q_obstacles: Query<(&Obstacle, &mut Transform)>,
    mut rm_rng: ResMut<BevyRng>,
) {
    let Ok((mut t, mut p)) = s_player.single_mut() else {
        return;
    };

    if p.dead {
        p.paused = true;
        for mut part in q_pipe_parts.iter_mut() {
            part.as_mut().visible = false;
        }
        info!("reset");
        reset_player(t.as_mut(), p.as_mut());
        for (i, (_, mut t)) in q_obstacles.iter_mut().enumerate() {
            // randomize_pipe_height
            t.translation.x =
                100. + (i as f32 / OBSTACLE_AMOUNT as f32) * (agb::display::WIDTH + 32) as f32;
            randomize_pipe_height(&mut rm_rng, &mut t);
        }
    }
}

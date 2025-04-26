#![expect(
    unsafe_code,
    reason = "sprite handle requires establishing safety invariants"
)]

use bevy::{
    platform::sync::{Arc, Weak},
    prelude::*,
};
use log::warn;

/// Sets up a rendering subsystem.
#[derive(Default)]
pub struct AgbRenderPlugin;

impl Plugin for AgbRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Last, render_objects);
    }

    fn finish(&self, app: &mut App) {
        let Some(display) = app
            .world_mut()
            .remove_non_send_resource::<agb::display::Display>()
        else {
            return;
        };

        let agb::display::Display {
            video,
            object,
            window,
            blend,
            ..
        } = display;

        let object = Box::leak(Box::new(object));

        let (oam, sprite_loader) = object.get_unmanaged();

        app.insert_non_send_resource(oam);
        app.insert_non_send_resource(sprite_loader);
        app.insert_non_send_resource(SpriteHandles::new());

        app.insert_resource(Video(video))
            .insert_resource(WindowDist(window))
            .insert_resource(BlendDist(blend));
    }
}

/// Asset storage for sprites.
#[derive(Default)]
pub struct SpriteHandles {
    sprites: Vec<Arc<agb::display::object::SpriteVram>>,
}

impl SpriteHandles {
    /// Create a new [`Sprites`].
    pub const fn new() -> Self {
        Self {
            sprites: Vec::new(),
        }
    }

    /// Gets a [`SpriteVram`](agb::display::object::SpriteVram) from storage by a [`SpriteHandle`].
    pub fn get(&self, handle: &SpriteHandle) -> Option<agb::display::object::SpriteVram> {
        handle
            .0
            .upgrade()
            .map(|arc| agb::display::object::SpriteVram::clone(&arc))
    }

    /// Adds a [`SpriteVram`](agb::display::object::SpriteVram) to storage for use by the rendering
    /// subsystem.
    pub fn add(&mut self, sprite: agb::display::object::SpriteVram) -> SpriteHandle {
        let arc = Arc::new(sprite);
        let handle = SpriteHandle(Arc::downgrade(&arc));
        self.sprites.push(arc);
        handle
    }
}

/// Handle to a stored sprite.
#[derive(Clone)]
pub struct SpriteHandle(Weak<agb::display::object::SpriteVram>);

impl PartialEq for SpriteHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

// SAFETY: `SpriteHandle` does not modify or read its contents
unsafe impl Send for SpriteHandle {}

// SAFETY: `SpriteHandle` does not modify or read its contents
unsafe impl Sync for SpriteHandle {}

/// Alternative to Bevy's `Sprite` type.
#[derive(Component, Clone)]
pub struct Sprite {
    /// Handle to the sprite graphics data.
    pub handle: SpriteHandle,
    /// Whether the sprite is horizontally flipped.
    pub horizontal_flipped: bool,
    /// Whether the sprite is vertically flipped.
    pub vertical_flipped: bool,
    /// Whether the sprite is visible.
    pub visible: bool,
    /// The draw priority of this sprite.
    pub priority: agb::display::Priority,
    /// The graphics mode of this sprite.
    pub graphics_mode: agb::display::object::GraphicsMode,
}

impl Sprite {
    /// Creates a new [`Sprite`] with the provided [`SpriteHandle`].
    pub const fn new(handle: SpriteHandle) -> Self {
        Self {
            handle,
            horizontal_flipped: false,
            vertical_flipped: false,
            visible: true,
            priority: agb::display::Priority::P0,
            graphics_mode: agb::display::object::GraphicsMode::Normal,
        }
    }
}

fn render_objects(
    mut oam: NonSendMut<agb::display::object::OamUnmanaged<'static>>,
    sprites: Query<(&Sprite, &GlobalTransform)>,
    sprite_assets: NonSendMut<SpriteHandles>,
) {
    let oam_iterator = &mut oam.iter();

    for (sprite, transform) in &sprites {
        let Some(handle) = sprite_assets.get(&sprite.handle) else {
            continue;
        };

        let mut obj = agb::display::object::ObjectUnmanaged::new(handle);

        if !sprite.visible {
            obj.hide();
            continue;
        }

        let Vec3 { x, y, .. } = transform.translation();

        let x = x.clamp(i32::MIN as f32, i32::MAX as f32) as i32;
        let y = y.clamp(i32::MIN as f32, i32::MAX as f32) as i32;

        if !(-64..240).contains(&x) && !(-64..160).contains(&y) {
            continue;
        }

        let position = agb::fixnum::Vector2D { x, y };

        obj.show()
            .set_position(position)
            .set_hflip(sprite.horizontal_flipped)
            .set_vflip(sprite.vertical_flipped)
            .set_priority(sprite.priority)
            .set_graphics_mode(sprite.graphics_mode);

        let Some(next) = oam_iterator.next() else {
            warn!("Ran out of OAM slots!");
            return;
        };

        next.set(&obj);
    }
}

/// Controls access to the underlying video hardware.
#[derive(Resource, Deref, DerefMut)]
pub struct Video(agb::display::video::Video);

/// Provides access to [`Windows`](agb::display::window::Windows).
#[derive(Resource, Deref, DerefMut)]
pub struct WindowDist(agb::display::WindowDist);

/// Provides access to [`Blend`](agb::display::blend::Blend).
#[derive(Resource, Deref, DerefMut)]
pub struct BlendDist(agb::display::BlendDist);

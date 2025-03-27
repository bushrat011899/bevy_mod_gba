use core::ops::{Deref, DerefMut};

use bevy::prelude::*;

/// Sets up the sound sub-system.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct AgbSoundPlugin {
    /// If `true`, will enable the sound subsystem on the DMG chip.
    /// Otherwise, you must [enable](agb::sound::dmg::Sound::enable) it yourself
    /// using the [`Sound`] resource.
    pub enable_dmg: bool,
    /// If `Some(...)`, will enable the mixer subsystem at the provided frequency.
    pub mixer_frequency: Option<agb::sound::mixer::Frequency>,
}

impl Plugin for AgbSoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            |mixer: Option<NonSendMut<agb::sound::mixer::Mixer>>| {
                if let Some(mut mixer) = mixer {
                    mixer.frame();
                }
            },
        );
    }

    fn finish(&self, app: &mut App) {
        if self.enable_dmg {
            if let Some(sound) = app
                .world_mut()
                .remove_non_send_resource::<agb::sound::dmg::Sound>()
            {
                sound.enable();

                let channel1 = Channel::<1>::from_sound(&sound);
                let channel2 = Channel::<2>::from_sound(&sound);
                let noise = Noise(sound.noise());

                app.insert_resource(channel1)
                    .insert_resource(channel2)
                    .insert_resource(noise);
            }
        }

        if let Some(frequency) = self.mixer_frequency {
            if let Some(mixer_controller) = app
                .world_mut()
                .remove_non_send_resource::<agb::sound::mixer::MixerController>()
            {
                let mixer_controller = Box::leak(Box::new(mixer_controller));

                let mut mixer = mixer_controller.mixer(frequency);

                mixer.enable();

                app.insert_non_send_resource(mixer);
            }
        }
    }
}

/// Manages access to the Game Boy Advance's beeps and boops sound hardware as part of the
/// original Game Boy's sound chip (the DMG).
#[derive(Resource, Deref, DerefMut)]
pub struct Sound(agb::sound::dmg::Sound);

/// Provides access to the noise generator.
/// See [`play_sound`](agb::sound::dmg::Noise::play_sound).
#[derive(Resource, Deref, DerefMut)]
pub struct Noise(agb::sound::dmg::Noise);

/// Provides access to the `N`th audio channel.
#[derive(Resource)]
pub struct Channel<const N: usize> {
    c1: agb::sound::dmg::Channel1,
    c2: agb::sound::dmg::Channel2,
}

impl Deref for Channel<1> {
    type Target = agb::sound::dmg::Channel1;

    fn deref(&self) -> &Self::Target {
        &self.c1
    }
}

impl DerefMut for Channel<1> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.c1
    }
}

impl Deref for Channel<2> {
    type Target = agb::sound::dmg::Channel2;

    fn deref(&self) -> &Self::Target {
        &self.c2
    }
}

impl DerefMut for Channel<2> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.c2
    }
}

impl Channel<1> {
    fn from_sound(sound: &agb::sound::dmg::Sound) -> Self {
        Self {
            c1: sound.channel1(),
            c2: sound.channel2(),
        }
    }
}

impl Channel<2> {
    fn from_sound(sound: &agb::sound::dmg::Sound) -> Self {
        Self {
            c1: sound.channel1(),
            c2: sound.channel2(),
        }
    }
}

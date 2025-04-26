use core::sync::atomic::Ordering;

use bevy::{platform::sync::atomic::AtomicBool, prelude::*};

use super::*;

/// Unpacks the [`Gba`](agb::Gba) data into an idiomatic form for Bevy.
///
/// # Safety
///
/// You must ensure no other code calls [`new_in_entry`](agb::Gba::new_in_entry).
#[derive(Default)]
pub struct AgbUnpackPlugin;

impl Plugin for AgbUnpackPlugin {
    fn build(&self, app: &mut App) {
        static UNPACKED: AtomicBool = AtomicBool::new(false);

        if UNPACKED.swap(true, Ordering::AcqRel) {
            return;
        }

        #[expect(unsafe_code, reason = "unpacking Gba is unsafe")]
        // SAFETY: UNPACKED ensures `new_in_entry` is only called once
        let gba = unsafe { agb::Gba::new_in_entry() };

        // Unpack agb structs into Bevy resources
        let agb::Gba {
            display,
            sound,
            mixer,
            save,
            timers,
            dma,
            ..
        } = gba;

        app.insert_non_send_resource(timers)
            .insert_non_send_resource(sound)
            .insert_non_send_resource(mixer)
            .insert_non_send_resource(save)
            .insert_non_send_resource(display)
            .insert_non_send_resource(dma);
    }
}

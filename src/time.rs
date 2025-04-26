use core::{sync::atomic::Ordering, time::Duration};

use bevy::{
    platform::{sync::atomic::AtomicU32, time::Instant},
    prelude::*,
};

/// Sets up [timers](Timer) 2 & 3 as [resources](Resource).
/// Uses [`Timer 2`](Timer) to provide [`Instant`](bevy::platform::time::Instant)
/// with reliable timing information.
#[derive(Default)]
pub struct AgbTimePlugin;

impl Plugin for AgbTimePlugin {
    fn build(&self, app: &mut App) {
        static INTERRUPTS: AtomicU32 = AtomicU32::new(0);

        #[expect(unsafe_code, reason = "setting up interrupts requires unsafe code")]
        // SAFETY: No allocation performed.
        let interrupt_handler = unsafe {
            agb::interrupt::add_interrupt_handler(agb::interrupt::Interrupt::Timer2, |_| {
                INTERRUPTS.add(1, Ordering::Release);
            })
        };

        app.insert_non_send_resource(interrupt_handler);

        const INTERRUPT_DURATION: Duration = Duration::from_nanos(1_000_000_000 >> 8);

        #[expect(unsafe_code, reason = "setting up Instant requires unsafe code")]
        // SAFETY:
        // - The function accurately represents the elapsed time.
        // - The function preserves all invariants of the `Instant` type.
        // - The pointer is statically valid
        unsafe {
            Instant::set_elapsed(|| INTERRUPTS.load(Ordering::Acquire) * INTERRUPT_DURATION);
        }
    }

    fn finish(&self, app: &mut App) {
        let Some(mut timers) = app
            .world_mut()
            .remove_non_send_resource::<agb::timer::TimerController>()
        else {
            return;
        };

        let agb::timer::Timers { timer2, timer3, .. } = timers.timers();

        let mut timer2 = Timer::<2>(timer2);
        let timer3 = Timer::<3>(timer3);

        timer2
            .set_enabled(true)
            .set_divider(agb::timer::Divider::Divider1)
            .set_overflow_amount(u16::MAX)
            .set_interrupt(true);

        app.insert_resource(timer2).insert_resource(timer3);
    }
}

/// Provides a single timer.
#[derive(Resource, Deref, DerefMut)]
pub struct Timer<const ID: u8>(agb::timer::Timer);

//! Provides integration between [`agb`] and [`bevy`].

#![no_std]

extern crate alloc;

mod audio;
mod dma;
mod input;
mod logging;
mod render;
mod runner;
mod save;
mod time;
mod unpack;

pub use agb;
pub use audio::*;
pub use dma::*;
pub use input::*;
pub use logging::*;
pub use render::*;
pub use runner::*;
pub use save::*;
pub use time::*;
pub use unpack::*;

use bevy::app::plugin_group;

plugin_group! {
    /// This plugin group will add all the default plugins for a Bevy application using [`agb`].
    pub struct AgbPlugin {
        :AgbUnpackPlugin,
        :AgbLogPlugin,
        :AgbInputPlugin,
        :AgbRenderPlugin,
        :AgbRunnerPlugin,
        :AgbTimePlugin,
        :AgbSavePlugin,
        :AgbSoundPlugin,
        :AgbDmaPlugin,
    }
}

pub mod prelude {
    //! Recommended imports.

    #[doc(hidden)]
    pub use crate::{AgbPlugin, Channel, Noise, SaveManager};
}

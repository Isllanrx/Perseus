#![cfg_attr(not(test), forbid(unsafe_code))]
#![cfg_attr(test, deny(unsafe_code))]

pub mod events;
pub mod features;
#[cfg(test)]
mod property_tests;
pub mod shared;
#[cfg(test)]
mod test_support;

pub use events::{Event, Level, Reporter};
pub use features::download::{DownloadOptions, DownloadReport, DownloadRequest, run_download};
pub use features::inspect::{InspectResult, TrackPreview, inspect_url};
pub use features::watch::{PlaylistWatcher, WatchConfig};
pub use shared::error::{Error, Result};
pub use shared::soundcloud::client::SoundCloudClient;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

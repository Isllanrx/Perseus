pub mod auth;
pub mod client;
pub mod models;
pub mod transcoding;
pub mod urls;

use crate::shared::config::{API_BASE, WEB_BASE};

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub api_base: String,
    pub web_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            api_base: API_BASE.to_owned(),
            web_base: WEB_BASE.to_owned(),
        }
    }
}

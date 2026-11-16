mod bridge;
mod deploy;

use std::sync::OnceLock;

use island_core::MusicConfig;

use super::integration::{GrantResult, Integration, IntegrationStatus};
use super::provider::MusicProvider;
use bridge::NcmBridgeProvider;

#[derive(Debug)]
pub struct Netease {
    bridge: OnceLock<NcmBridgeProvider>,
}

pub static NETEASE: Netease = Netease {
    bridge: OnceLock::new(),
};

impl Integration for Netease {
    fn id(&self) -> &'static str {
        "netease"
    }

    fn enabled(&self, music: &MusicConfig) -> bool {
        music.netease_bridge
    }

    fn provider(&'static self, on_change: fn()) -> &'static dyn MusicProvider {
        self.bridge
            .get_or_init(|| NcmBridgeProvider::start(on_change))
    }

    fn set_wanted(&self, want: bool) {
        deploy::set_wanted(want);
    }

    fn status(&self) -> IntegrationStatus {
        let connected = self
            .bridge
            .get()
            .is_some_and(NcmBridgeProvider::is_connected);
        deploy::status(connected)
    }

    fn grant(&self) -> GrantResult {
        deploy::grant_access()
    }
}

#![cfg(test)]

mod support;

use headphone_bridge_core::Core;
use support::fake_platform;

#[tokio::test]
async fn with_no_device_connected_the_device_list_is_empty() {
    let (platform, _os) = fake_platform();

    let core = Core::start(platform);

    assert!(core.device_list().devices.is_empty());
}

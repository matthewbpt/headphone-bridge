#![cfg(test)]

use std::error::Error;

use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_core::CommandError;

#[test]
fn link_failures_and_command_errors_are_errors_with_developer_text() {
    let errors: [Box<dyn Error>; 2] = [
        Box::new(LinkFailure::Other("secure: IOException".into())),
        Box::new(CommandError::FrameLogUnavailable(
            "permission denied".into(),
        )),
    ];
    assert_eq!(
        errors.map(|error| error.to_string()),
        [
            "secure: IOException",
            "couldn't open the frame log: permission denied"
        ]
    );
}

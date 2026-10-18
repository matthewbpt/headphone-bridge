//! The JNI symbols for `NativeCore` in the platform-android crate's Kotlin
//! library. They live in this crate, the cdylib, so the linker keeps them,
//! and so the widget and tile can have the shell's labels.

use std::path::PathBuf;

use headphone_bridge_platform_android::android;
use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use crate::labels;

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_start(
    mut env: JNIEnv,
    class: JClass,
    frame_log: JString,
) {
    let frame_log = (!frame_log.is_null())
        .then(|| env.get_string(&frame_log).ok().map(String::from))
        .flatten()
        .map(PathBuf::from);
    log_failure(android::start(&mut env, &class, frame_log.as_deref()));
}

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_event(
    mut env: JNIEnv,
    _class: JClass,
    json: JString,
) {
    if let Ok(json) = env.get_string(&json) {
        log_failure(android::event(&String::from(json)));
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_command(
    mut env: JNIEnv,
    _class: JClass,
    request: jlong,
    json: JString,
) {
    if let Ok(json) = env.get_string(&json) {
        log_failure(android::command(request, &String::from(json)));
    }
}

/// Logs why the core couldn't start, since a JNI call can't return it.
/// Kotlin carries on without the core, and the next call tries again.
fn log_failure(result: Result<(), android::StartError>) {
    if let Err(error) = result {
        let source = std::error::Error::source(&error)
            .map(|source| format!(": {source}"))
            .unwrap_or_default();
        android::log(&format!("{error}{source}"));
    }
}

/// The labels as JSON, as the window gets them, for the widget and tile;
/// null, logged, if they can't be handed across.
#[no_mangle]
pub extern "system" fn Java_dev_matthewbpt_headphonebridge_bluetooth_NativeCore_labels(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = match serde_json::to_string(&labels::all()) {
        Ok(json) => json,
        Err(e) => {
            android::log(&format!("couldn't serialise the labels: {e}"));
            return std::ptr::null_mut();
        }
    };
    match env.new_string(json) {
        Ok(json) => json.into_raw(),
        Err(e) => {
            android::log(&format!("couldn't hand the labels across: {e}"));
            std::ptr::null_mut()
        }
    }
}

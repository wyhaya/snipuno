use crate::Status;
use objc2::rc::Retained;
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSBundle};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

fn service() -> Result<Retained<SMAppService>, String> {
    if NSBundle::mainBundle()
        .bundleIdentifier()
        .is_none_or(|identifier| identifier.to_string() != "com.snipuno.desktop")
    {
        return Err("Launch at login is available when running the Snipuno app bundle.".into());
    }
    Ok(unsafe { SMAppService::mainAppService() })
}

pub fn status() -> Result<Status, String> {
    match unsafe { service()?.status() } {
        SMAppServiceStatus::Enabled => Ok(Status::Enabled),
        SMAppServiceStatus::RequiresApproval => Ok(Status::RequiresApproval),
        SMAppServiceStatus::NotRegistered | SMAppServiceStatus::NotFound => Ok(Status::Disabled),
        _ => Err("Unable to determine the launch at login status.".into()),
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let current = status()?;
    let service = service()?;
    let result = if enabled && current == Status::Disabled {
        unsafe { service.registerAndReturnError() }
    } else if !enabled && current != Status::Disabled {
        unsafe { service.unregisterAndReturnError() }
    } else {
        return Ok(());
    };
    if enabled && status() == Ok(Status::RequiresApproval) {
        return Ok(());
    }
    result.map_err(|error| error.localizedDescription().to_string())
}

pub fn open_settings() -> Result<(), String> {
    unsafe { SMAppService::openSystemSettingsLoginItems() };
    Ok(())
}

/// Call during `applicationDidFinishLaunching` while the launch Apple event is current.
pub fn was_launched_at_login() -> bool {
    NSAppleEventManager::sharedAppleEventManager()
        .currentAppleEvent()
        .is_some_and(|event| is_login_event(&event))
}

fn is_login_event(event: &NSAppleEventDescriptor) -> bool {
    event.eventID() == u32::from_be_bytes(*b"oapp")
        && event
            .paramDescriptorForKeyword(u32::from_be_bytes(*b"prdt"))
            .is_some_and(|value| value.enumCodeValue() == u32::from_be_bytes(*b"lgit"))
}

//! Active Core Audio endpoints, grouped by their exact adapter PnP instance ID.

use crate::{hw::Ctx, report::Out, win};
use win::audio::Endpoint;

/// Collects audio identities without activating streams or changing device state.
pub fn collect(ctx: &Ctx, out: &mut Out) -> win::Result<()> {
    let scan = win::audio::endpoints()?;
    out.source("Core Audio");
    for error in &scan.failures {
        out.fallback_failed("Core Audio", error);
    }
    for (direction, count) in scan.inactive {
        out.fallback_failed(
            "Core Audio inactive endpoints (diagnostic only)",
            &win::Error::msg(
                "audio inventory",
                format!("{direction}: {count} inactive endpoints excluded"),
            ),
        );
    }
    if scan.endpoints.is_empty() {
        if scan.incomplete {
            return Err(win::Error::msg(
                "audio inventory",
                "active endpoint enumeration incomplete",
            ));
        }
        out.text("No active audio endpoints detected.");
        return Ok(());
    }

    let hardware_ids = match ctx.hardware_ids() {
        Ok(map) => Some(map),
        Err(error) => {
            out.fallback_failed("SetupAPI hardware IDs", &error);
            None
        }
    };
    let devices = match win::setupapi::DevInfoSet::enum_present_all() {
        Ok(set) => Some(set),
        Err(error) => {
            out.fallback_failed("SetupAPI ContainerID", &error);
            None
        }
    };
    if hardware_ids.is_some() || devices.is_some() {
        out.source("Core Audio + SetupAPI");
    }
    let mut endpoints = scan.endpoints;
    endpoints.sort_by_key(|endpoint| {
        (
            endpoint.instance_id.is_none(),
            endpoint.instance_id.as_ref().map(|id| id.to_uppercase()),
            endpoint.direction,
            endpoint.name.clone(),
            endpoint.id.clone(),
        )
    });
    let mut groups: Vec<Vec<Endpoint>> = Vec::new();
    for endpoint in endpoints {
        // Names alone cannot distinguish identical adapters. All unresolved
        // adapters share the final group; exact IDs join case-insensitively.
        let group = endpoint
            .instance_id
            .as_ref()
            .and_then(|id| {
                groups.iter().position(|group| {
                    group[0]
                        .instance_id
                        .as_ref()
                        .is_some_and(|other| other.eq_ignore_ascii_case(id))
                })
            })
            .or_else(|| {
                endpoint
                    .instance_id
                    .is_none()
                    .then(|| {
                        groups
                            .iter()
                            .position(|group| group[0].instance_id.is_none())
                    })
                    .flatten()
            });
        match group {
            Some(index) => groups[index].push(endpoint),
            None => groups.push(vec![endpoint]),
        }
    }
    for (index, group) in groups.iter().enumerate() {
        if index != 0 {
            out.separator();
        }
        let instance_id = group[0].instance_id.as_deref();
        let hardware_id = instance_id.and_then(|id| hardware_ids?.get(&id.to_uppercase()));
        if instance_id.is_some() && hardware_ids.is_some() && hardware_id.is_none() {
            out.fallback_failed(
                "SetupAPI hardware IDs",
                &win::Error::msg(
                    "audio adapter hardware IDs",
                    "no matching hardware ID in snapshot",
                ),
            );
        }
        let container_id = match (devices.as_ref(), instance_id) {
            (Some(set), Some(id)) => match set.container_id(id) {
                Ok(value) => {
                    if value.is_none() {
                        out.fallback_failed(
                            "SetupAPI ContainerID",
                            &win::Error::msg(
                                "audio adapter ContainerID",
                                "present devnode/property absent or null GUID",
                            ),
                        );
                    }
                    value
                }
                Err(error) => {
                    out.fallback_failed("SetupAPI ContainerID", &error);
                    None
                }
            },
            _ => None,
        };
        render_adapter(
            out,
            group,
            hardware_id.map(String::as_str),
            container_id.as_deref(),
            devices.as_ref(),
        );
    }
    Ok(())
}

fn render_adapter(
    out: &mut Out,
    endpoints: &[Endpoint],
    hardware_id: Option<&str>,
    container_id: Option<&str>,
    devices: Option<&win::setupapi::DevInfoSet>,
) {
    let adapter = if endpoints[0].instance_id.is_some() {
        endpoints
            .iter()
            .find_map(|e| e.adapter.as_deref())
            .unwrap_or("Unknown Audio Adapter")
    } else {
        "Unresolved Audio Adapters"
    };
    out.info("Adapter", adapter);
    if let Some(id) = endpoints[0].instance_id.as_deref() {
        out.id("Instance ID", id);
    }
    if let Some(id) = hardware_id {
        out.id("Hardware IDs", id);
    }
    if let Some(id) = container_id {
        out.id("Container ID", id);
    }
    for endpoint in endpoints {
        out.info(
            "Endpoint",
            endpoint.name.as_deref().unwrap_or("Unknown Audio Endpoint"),
        );
        out.info("Direction", endpoint.direction);
        if let Some(id) = endpoint.id.as_deref() {
            // The opaque endpoint token includes a GUID; mask the entire value.
            out.id("Endpoint ID", id);
            if container_id.is_none()
                && let Some(set) = devices
            {
                // The documented endpoint devnode is separate from its KS adapter.
                // Label its container at the endpoint, never as an adapter container.
                match set.container_id(&format!("SWD\\MMDEVAPI\\{id}")) {
                    Ok(Some(container)) => {
                        out.id("Endpoint Container ID", &container);
                    }
                    Ok(None) => {
                        out.fallback_failed(
                            "SetupAPI endpoint ContainerID",
                            &win::Error::msg(
                                "audio endpoint ContainerID",
                                "present devnode/property absent or null GUID",
                            ),
                        );
                    }
                    Err(error) => {
                        out.fallback_failed("SetupAPI endpoint ContainerID", &error);
                    }
                }
            }
        }
        if let Some(id) = endpoint.stable_id.as_deref() {
            out.id("Stable ID", id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_audio_identity_is_masked_without_hiding_names() {
        let endpoint = Endpoint {
            direction: "Render",
            id: Some("{0.0.0.00000000}.{6d73e082-684f-4130-a0cb-fb538d1c3279}".into()),
            name: Some("Speakers (USB Audio Device)".into()),
            adapter: Some("USB Audio Device".into()),
            instance_id: Some(r"USB\VID_046D&PID_0A9F\A7C28E41".into()),
            stable_id: Some("audio-device-Q7f29B4a".into()),
        };
        let mut out = Out::new();
        render_adapter(
            &mut out,
            &[endpoint],
            Some(r"USB\VID_046D&PID_0A9F&REV_0100"),
            Some("{52B18C39-7D64-4AF0-963E-826A19DB4507}"),
            None,
        );
        let raw = out.finish();
        assert_eq!(raw.ids.len(), 5);
        let masked = crate::report::masked(&raw);
        for id in &raw.ids {
            assert!(id.chars().count() >= 4);
            assert!(!masked.body.contains(id));
            assert!(masked.body.contains(&crate::report::mask_value(id)));
        }
        assert!(masked.body.contains("Speakers (USB Audio Device)\r\n"));
        assert!(masked.body.contains("Direction: Render\r\n"));
    }
}
